pub mod parser;

pub use parser::{
    canonical_hash,
    canonical_text,
    parse,
    parse_file,
    resolve_imports_from_file,
    Definition,
    Instance,
    MemoryBlock,
    Module,
};
