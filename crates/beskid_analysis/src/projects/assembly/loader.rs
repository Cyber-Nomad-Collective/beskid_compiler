//! Discover, parse, and index compilation units for a compile plan.

mod discovery;
mod options;
mod orchestration;
mod scanner;
mod trusted_paths;

#[cfg(test)]
mod tests;

use super::roots;

pub(crate) use self::options::expand_syntax_for_assembly;
pub use self::options::{assembly_options_for_plan, assembly_options_for_prepare, AssemblyError, UnitMaterializer};
pub(crate) use self::orchestration::assemble_program;
pub use self::orchestration::assemble_program_with_materializer;
pub(crate) use self::scanner::{
    import_paths_from_source_full, module_paths_from_qualified_references, parent_module_import_path,
};
