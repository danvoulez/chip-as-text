use chip_as_text::{canonical_hash, canonical_text, parse_file};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        print_usage();
        return;
    }

    let command = args[1].as_str();
    let path = &args[2];
    let flags = &args[3..];

    match command {
        "parse" => match parse_file(path) {
            Ok(def) => {
                if flags.iter().any(|flag| flag == "--json") {
                    match serde_json::to_string_pretty(&def) {
                        Ok(json) => println!("{}", json),
                        Err(err) => eprintln!("JSON serialization error: {}", err),
                    }
                    return;
                }

                if flags.iter().any(|flag| flag == "--canonical") {
                    println!("{}", canonical_text(&def));
                    return;
                }

                println!("Parsed successfully!");
                println!("Kind: {}", def.kind);
                println!("Name: {}", def.name);
                if let Some(full) = &def.full_name {
                    println!("Full Name: {}", full);
                }
                println!("Canonical Hash: {}", canonical_hash(&def));
                println!("Modules: {}", def.modules.len());
                println!("Instances: {}", def.instantiate.len());
                if !def.memory_blocks.is_empty() {
                    println!("Memory Blocks: {}", def.memory_blocks.len());
                }
            }
            Err(e) => eprintln!("Parse error: {}", e),
        },
        "hash" => match parse_file(path) {
            Ok(def) => println!("{}", canonical_hash(&def)),
            Err(e) => eprintln!("Parse error: {}", e),
        },
        _ => print_usage(),
    }
}

fn print_usage() {
    println!("Usage:");
    println!("  chip parse <file>");
    println!("  chip parse <file> --json");
    println!("  chip parse <file> --canonical");
    println!("  chip hash <file>");
}
