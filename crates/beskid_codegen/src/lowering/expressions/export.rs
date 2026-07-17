//! `[Export]` metadata on `pub` functions for AOT linker-visible symbols.

use beskid_analysis::hir::{HirFunctionDefinition, HirVisibility};
use beskid_analysis::syntax::Spanned;
use cranelift_codegen::ir::Signature;

use crate::cranelift_host::validate_ffi_signature;
use crate::errors::CodegenError;

/// One exported Beskid function and its linker-visible symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportEntry {
    pub beskid_name: String,
    pub exported_symbol: String,
    pub abi: String,
}

/// Read `[Export(Abi: "...", Symbol: "...")]` from a lowered function definition.
pub fn read_export_metadata(def: &Spanned<HirFunctionDefinition>) -> Option<ExportEntry> {
    let export = def.node.export_interface.as_ref()?;
    let beskid_name = def.node.name.node.name.clone();
    let exported_symbol = export.symbol.clone().unwrap_or_else(|| beskid_name.clone());
    let abi = export.abi.clone().unwrap_or_else(|| "C".to_string());
    Some(ExportEntry {
        beskid_name,
        exported_symbol,
        abi,
    })
}

/// Validate export placement and FFI signature for `def`.
pub fn validate_export_function(
    def: &Spanned<HirFunctionDefinition>,
    signature: &Signature,
    pointer: cranelift_codegen::ir::Type,
) -> Result<Option<ExportEntry>, CodegenError> {
    let Some(entry) = read_export_metadata(def) else {
        return Ok(None);
    };

    if !matches!(def.node.visibility.node, HirVisibility::Public) {
        return Err(CodegenError::InvalidExport {
            span: def.span,
            message: "`[Export]` applies to `pub` functions only".to_owned(),
        });
    }

    if entry.abi != "C" {
        return Err(CodegenError::InvalidExport {
            span: def.span,
            message: format!(
                "unsupported export ABI `{}` (v0.3 supports `C` only)",
                entry.abi
            ),
        });
    }

    validate_ffi_signature(signature, pointer).map_err(|msg| CodegenError::InvalidExport {
        span: def.span,
        message: format!(
            "export signature not allowed for {}: {msg}",
            entry.exported_symbol
        ),
    })?;

    Ok(Some(entry))
}

/// Collect export entries from top-level and inline-module function items.
pub fn collect_exports(items: &[Spanned<beskid_analysis::hir::HirItem>]) -> Vec<ExportEntry> {
    let mut out = Vec::new();
    collect_exports_from_items(items, &mut out);
    out
}

fn collect_exports_from_items(
    items: &[Spanned<beskid_analysis::hir::HirItem>],
    out: &mut Vec<ExportEntry>,
) {
    use beskid_analysis::hir::{HirInlineModule, HirItem};

    for item in items {
        match &item.node {
            HirItem::FunctionDefinition(def) => {
                if let Some(entry) = read_export_metadata(def) {
                    out.push(entry);
                }
            }
            HirItem::InlineModule(module) => {
                let module: &Spanned<HirInlineModule> = module;
                collect_exports_from_items(&module.node.items, out);
            }
            _ => {}
        }
    }
}

/// Resolve the Cranelift symbol name for a function that may carry `[Export(Symbol: "...")]`.
pub fn export_linker_name(def: &Spanned<HirFunctionDefinition>) -> String {
    read_export_metadata(def)
        .map(|e| e.exported_symbol)
        .unwrap_or_else(|| def.node.name.node.name.clone())
}

/// Native object-file symbol for AOT linking (`Main` maps to C `main` for executable entry).
pub fn object_link_symbol(beskid_name: &str, exports: &[ExportEntry]) -> String {
    let logical = beskid_name.split('#').next().unwrap_or(beskid_name);
    if let Some(entry) = exports
        .iter()
        .find(|e| e.beskid_name == logical || e.beskid_name == beskid_name)
    {
        return entry.exported_symbol.clone();
    }
    if logical == "Main" {
        return "main".to_string();
    }
    beskid_name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_analysis::hir::HirExportInterface;

    #[test]
    fn object_link_symbol_maps_main_entry_to_native_main() {
        assert_eq!(object_link_symbol("Main", &[]), "main");
        assert_eq!(object_link_symbol("Main#74", &[]), "main");
        assert_eq!(object_link_symbol("Run", &[]), "Run");
    }

    #[test]
    fn read_export_defaults_symbol_to_beskid_name() {
        let def = Spanned::new(
            HirFunctionDefinition {
                export_interface: Some(HirExportInterface {
                    abi: Some("C".into()),
                    symbol: None,
                }),
                runtime_handler: None,
                attributes: Vec::new(),
                visibility: Spanned::new(HirVisibility::Public, Default::default()),
                name: Spanned::new(
                    beskid_analysis::hir::HirIdentifier {
                        name: "plugin_init".into(),
                    },
                    Default::default(),
                ),
                generics: Vec::new(),
                parameters: Vec::new(),
                return_type: None,
                body: Spanned::new(
                    beskid_analysis::hir::HirBlock {
                        statements: Vec::new(),
                    },
                    Default::default(),
                ),
            },
            Default::default(),
        );
        let entry = read_export_metadata(&def).expect("export metadata");
        assert_eq!(entry.exported_symbol, "plugin_init");
        assert_eq!(entry.abi, "C");
    }
}
