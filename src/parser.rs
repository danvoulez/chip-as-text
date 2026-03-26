use std::fs;
use std::path::{Path, PathBuf};

use blake3;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Definition {
    pub kind: String,
    pub name: String,
    pub full_name: Option<String>,
    pub description: String,
    pub goals: Vec<String>,
    pub modules: Vec<Module>,
    pub instantiate: Vec<Instance>,
    pub connect: Vec<String>,
    pub memory_blocks: Vec<MemoryBlock>,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Module {
    pub name: String,
    pub summary: Option<String>,
    pub operations: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Instance {
    pub module_name: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryBlock {
    pub name: String,
    pub size: String,
}

pub fn parse(content: &str) -> Result<Definition, String> {
    let lines: Vec<&str> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    let mut def = Definition {
        kind: "CHIP".to_string(),
        name: String::new(),
        full_name: None,
        description: String::new(),
        goals: vec![],
        modules: vec![],
        instantiate: vec![],
        connect: vec![],
        memory_blocks: vec![],
        output: String::new(),
    };

    let mut current_section = "";
    let mut current_module: Option<Module> = None;
    let mut header_count = 0_u32;

    for line in lines {
        if line.starts_with("IMPORT ") {
            return Err(
                "Unresolved IMPORT directive found in parse(). Use parse_file() for hierarchical imports."
                    .to_string(),
            );
        }

        if line.starts_with("# CHIP v2") {
            header_count += 1;
            if header_count > 1 {
                return Err("Multiple definition headers found. Imported files should be fragments without their own # CHIP v2 / # MEMORY v2 header.".to_string());
            }
            def.kind = "CHIP".to_string();
            def.name = line
                .strip_prefix("# CHIP v2")
                .unwrap_or("")
                .trim()
                .to_string();
            continue;
        }
        if line.starts_with("# MEMORY v2") {
            header_count += 1;
            if header_count > 1 {
                return Err("Multiple definition headers found. Imported files should be fragments without their own # CHIP v2 / # MEMORY v2 header.".to_string());
            }
            def.kind = "MEMORY".to_string();
            def.name = line
                .strip_prefix("# MEMORY v2")
                .unwrap_or("")
                .trim()
                .to_string();
            continue;
        }

        if line.starts_with("Full Name:") {
            def.full_name = Some(line.replace("Full Name:", "").trim().to_string());
            continue;
        }
        if line.starts_with("Description:") {
            flush_current_module(&mut current_module, &mut def.modules);
            current_section = "description";
            continue;
        }
        if line.starts_with("Architecture Goals:") {
            flush_current_module(&mut current_module, &mut def.modules);
            current_section = "goals";
            continue;
        }
        if line.starts_with("Modules:") {
            flush_current_module(&mut current_module, &mut def.modules);
            current_section = "modules";
            continue;
        }
        if line.starts_with("Instantiate:") {
            flush_current_module(&mut current_module, &mut def.modules);
            current_section = "instantiate";
            continue;
        }
        if line.starts_with("Connect:") {
            flush_current_module(&mut current_module, &mut def.modules);
            current_section = "connect";
            continue;
        }
        if line.starts_with("Memory:") {
            flush_current_module(&mut current_module, &mut def.modules);
            current_section = "memory";
            continue;
        }
        if line.starts_with("Output:") {
            flush_current_module(&mut current_module, &mut def.modules);
            current_section = "";
            def.output = line.replace("Output:", "").trim().to_string();
            continue;
        }

        match current_section {
            "description" => {
                if !def.description.is_empty() {
                    def.description.push('\n');
                }
                def.description.push_str(line);
            }
            "goals" => {
                if let Some(goal) = line.strip_prefix("- ") {
                    def.goals.push(goal.trim().to_string());
                }
            }
            "modules" => parse_module_line(line, &mut current_module, &mut def.modules),
            "instantiate" => parse_instantiate_line(line, &mut def.instantiate),
            "connect" => def.connect.push(line.to_string()),
            "memory" => parse_memory_line(line, &mut def.memory_blocks),
            _ => {}
        }
    }

    flush_current_module(&mut current_module, &mut def.modules);

    if def.name.is_empty() {
        return Err("Missing header: # CHIP v2 or # MEMORY v2".to_string());
    }

    Ok(def)
}

pub fn parse_file(path: impl AsRef<Path>) -> Result<Definition, String> {
    let resolved = resolve_imports_from_file(path)?;
    parse(&resolved)
}

pub fn resolve_imports_from_file(path: impl AsRef<Path>) -> Result<String, String> {
    let path = path.as_ref();
    let canonical = path.canonicalize().map_err(|e| {
        format!(
            "Failed to canonicalize '{}': {}",
            path.display(),
            e
        )
    })?;

    let mut stack = Vec::<PathBuf>::new();
    resolve_imports_inner(&canonical, &mut stack)
}

fn resolve_imports_inner(path: &Path, stack: &mut Vec<PathBuf>) -> Result<String, String> {
    if stack.iter().any(|p| p == path) {
        let mut cycle = stack
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>();
        cycle.push(path.display().to_string());
        return Err(format!("Import cycle detected: {}", cycle.join(" -> ")));
    }

    stack.push(path.to_path_buf());

    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read '{}': {}", path.display(), e))?;
    let base_dir = path
        .parent()
        .ok_or_else(|| format!("Could not determine parent directory for '{}'", path.display()))?;

    let mut output = String::new();

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        if let Some(import_target) = parse_import_target(trimmed) {
            let import_path = base_dir.join(import_target);
            let canonical_import = import_path.canonicalize().map_err(|e| {
                format!(
                    "Failed to resolve import '{}' from '{}': {}",
                    import_path.display(),
                    path.display(),
                    e
                )
            })?;

            let imported = resolve_imports_inner(&canonical_import, stack)?;
            output.push_str(&imported);
            if !imported.ends_with('\n') {
                output.push('\n');
            }
        } else {
            output.push_str(raw_line);
            output.push('\n');
        }
    }

    stack.pop();
    Ok(output)
}

fn parse_import_target(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("IMPORT ")?.trim();
    if rest.is_empty() {
        return None;
    }

    if rest.len() >= 2
        && ((rest.starts_with('"') && rest.ends_with('"'))
            || (rest.starts_with('\'') && rest.ends_with('\'')))
    {
        return Some(&rest[1..rest.len() - 1]);
    }

    Some(rest)
}

fn flush_current_module(current: &mut Option<Module>, modules: &mut Vec<Module>) {
    if let Some(module) = current.take() {
        modules.push(module);
    }
}

fn parse_module_line(line: &str, current: &mut Option<Module>, modules: &mut Vec<Module>) {
    if line.starts_with("Define module ") {
        flush_current_module(current, modules);
        let name = line
            .replace("Define module ", "")
            .replace(':', "")
            .trim()
            .to_string();
        *current = Some(Module {
            name,
            summary: None,
            operations: vec![],
            inputs: vec![],
            outputs: vec![],
        });
    } else if let Some(m) = current.as_mut() {
        if line.starts_with("Summary:") {
            m.summary = Some(line.replace("Summary:", "").trim().to_string());
        } else if line.starts_with("Operations:") {
            m.operations = line
                .replace("Operations:", "")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        } else if line.starts_with("Inputs:") {
            m.inputs = line
                .replace("Inputs:", "")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        } else if line.starts_with("Outputs:") {
            m.outputs = line
                .replace("Outputs:", "")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
}

fn parse_instantiate_line(line: &str, instantiate: &mut Vec<Instance>) {
    let Some(rest) = line.strip_prefix("Create ") else {
        return;
    };

    let Some((count_str, module_name)) = rest.split_once(" instances of ") else {
        return;
    };

    let Ok(count) = count_str.trim().parse::<u32>() else {
        return;
    };

    let module_name = module_name.trim();
    if module_name.is_empty() {
        return;
    }

    instantiate.push(Instance {
        module_name: module_name.to_string(),
        count,
    });
}

fn parse_memory_line(line: &str, blocks: &mut Vec<MemoryBlock>) {
    if let Some((name, size)) = line.split_once(':') {
        blocks.push(MemoryBlock {
            name: name.trim().to_string(),
            size: size.trim().to_string(),
        });
    }
}

pub fn canonical_text(def: &Definition) -> String {
    let mut out = String::new();

    out.push_str(&format!("kind:{}\n", escape_value(&def.kind)));
    out.push_str(&format!("name:{}\n", escape_value(&def.name)));
    out.push_str(&format!(
        "full_name:{}\n",
        escape_value(def.full_name.as_deref().unwrap_or(""))
    ));
    out.push_str(&format!("description:{}\n", escape_value(&def.description)));

    for goal in &def.goals {
        out.push_str(&format!("goal:{}\n", escape_value(goal)));
    }

    for module in &def.modules {
        out.push_str("module_begin\n");
        out.push_str(&format!("module_name:{}\n", escape_value(&module.name)));
        out.push_str(&format!(
            "module_summary:{}\n",
            escape_value(module.summary.as_deref().unwrap_or(""))
        ));
        for operation in &module.operations {
            out.push_str(&format!("module_operation:{}\n", escape_value(operation)));
        }
        for input in &module.inputs {
            out.push_str(&format!("module_input:{}\n", escape_value(input)));
        }
        for output in &module.outputs {
            out.push_str(&format!("module_output:{}\n", escape_value(output)));
        }
        out.push_str("module_end\n");
    }

    for instance in &def.instantiate {
        out.push_str(&format!(
            "instance:{}:{}\n",
            instance.count,
            escape_value(&instance.module_name)
        ));
    }

    for connection in &def.connect {
        out.push_str(&format!("connect:{}\n", escape_value(connection)));
    }

    for block in &def.memory_blocks {
        out.push_str(&format!(
            "memory:{}:{}\n",
            escape_value(&block.name),
            escape_value(&block.size)
        ));
    }

    out.push_str(&format!("output:{}\n", escape_value(&def.output)));
    out
}

pub fn canonical_hash(def: &Definition) -> String {
    let hash = blake3::hash(canonical_text(def).as_bytes());
    hash.to_hex().to_string()
}

fn escape_value(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace(':', "\\:")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{canonical_hash, parse, parse_file, resolve_imports_from_file};

    #[test]
    fn parses_chip_example() {
        let input = include_str!("../examples/blackwell-sm.chip");
        let def = parse(input).expect("blackwell-sm should parse");

        assert_eq!(def.kind, "CHIP");
        assert_eq!(def.name, "blackwell-sm");
        assert_eq!(def.modules.len(), 3);
        assert_eq!(def.instantiate.len(), 3);
        assert_eq!(def.instantiate[0].module_name, "Warp Scheduler");
        assert!(!canonical_hash(&def).is_empty());
    }

    #[test]
    fn parses_memory_example() {
        let input = include_str!("../examples/sm-memory.chip");
        let def = parse(input).expect("sm-memory should parse");

        assert_eq!(def.kind, "MEMORY");
        assert_eq!(def.name, "sm-memory");
        assert_eq!(def.memory_blocks.len(), 3);
        assert_eq!(def.instantiate[0].module_name, "Memory Controller");
    }

    #[test]
    fn rejects_missing_header() {
        let err = parse("Description:\nMissing header").unwrap_err();
        assert!(err.contains("Missing header"));
    }

    #[test]
    fn hash_changes_when_structure_changes() {
        let a = parse(include_str!("../examples/blackwell-sm.chip")).unwrap();
        let b = parse_file("examples/blackwell-sm-imported.chip").unwrap();

        assert_ne!(canonical_hash(&a), canonical_hash(&b));
    }

    #[test]
    fn resolves_imports_recursively() {
        let dir = temp_test_dir();
        let root = dir.join("root.chip");
        let nested_dir = dir.join("fragments");
        fs::create_dir_all(&nested_dir).unwrap();

        fs::write(
            nested_dir.join("ops.chipfrag"),
            "Operations: add, mul\nInputs: a, b\nOutputs: c\n",
        )
        .unwrap();

        fs::write(
            nested_dir.join("module.chipfrag"),
            "Define module ALU:\nSummary: Arithmetic core.\nIMPORT ops.chipfrag\n",
        )
        .unwrap();

        fs::write(
            &root,
            "# CHIP v2 imported-test\nDescription:\nImported description.\nModules:\nIMPORT fragments/module.chipfrag\nOutput: done\n",
        )
        .unwrap();

        let resolved = resolve_imports_from_file(&root).unwrap();
        assert!(resolved.contains("Define module ALU:"));
        assert!(resolved.contains("Operations: add, mul"));

        let parsed = parse_file(&root).unwrap();
        assert_eq!(parsed.modules.len(), 1);
        assert_eq!(parsed.modules[0].name, "ALU");
        assert_eq!(parsed.modules[0].operations, vec!["add", "mul"]);
    }

    #[test]
    fn rejects_multiple_headers_after_import_resolution() {
        let dir = temp_test_dir();
        let root = dir.join("root.chip");
        let imported = dir.join("imported.chip");

        fs::write(&imported, "# MEMORY v2 secondary\nDescription:\nNope\nOutput: x\n").unwrap();
        fs::write(
            &root,
            "# CHIP v2 primary\nDescription:\nRoot\nIMPORT imported.chip\nOutput: y\n",
        )
        .unwrap();

        let err = parse_file(&root).unwrap_err();
        assert!(err.contains("Multiple definition headers"));
    }

    #[test]
    fn rejects_import_cycles() {
        let dir = temp_test_dir();
        let a = dir.join("a.chip");
        let b = dir.join("b.chipfrag");

        fs::write(&a, "# CHIP v2 cycle\nDescription:\nA\nIMPORT b.chipfrag\nOutput: z\n").unwrap();
        fs::write(&b, "IMPORT a.chip\n").unwrap();

        let err = parse_file(&a).unwrap_err();
        assert!(err.contains("Import cycle detected"));
    }

    fn temp_test_dir() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("chip_as_text_test_{}", nonce));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
