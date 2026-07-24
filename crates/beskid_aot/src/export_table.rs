//! Export metadata collected from [`beskid_codegen::CodegenArtifact::exports`].

use beskid_codegen::CodegenArtifact;
use beskid_codegen::ExportEntry;

use crate::api::ExportPolicy;

/// Linker-visible export row derived from codegen export metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportTableEntry {
    pub symbol_id: u32,
    pub beskid_name: String,
    pub exported_symbol: String,
    pub abi: String,
}

/// Export table built from a codegen artifact.
#[derive(Debug, Clone, Default)]
pub struct ExportTable {
    entries: Vec<ExportTableEntry>,
}

impl ExportTable {
    pub fn from_artifact(artifact: &CodegenArtifact) -> Self {
        Self {
            entries: artifact
                .exports
                .iter()
                .enumerate()
                .map(|(index, entry)| ExportTableEntry::from_codegen(index as u32 + 1, entry))
                .collect(),
        }
    }

    pub fn entries(&self) -> &[ExportTableEntry] {
        &self.entries
    }

    pub fn linker_symbols(&self) -> Vec<String> {
        self.entries.iter().map(|entry| entry.exported_symbol.clone()).collect()
    }

    pub fn symbol_id_for_exported_symbol(&self, exported_symbol: &str) -> Option<u32> {
        self.entries.iter().find(|entry| entry.exported_symbol == exported_symbol).map(|entry| entry.symbol_id)
    }

    pub fn resolve_export_policy(&self, base: &ExportPolicy) -> ExportPolicy {
        if self.entries.is_empty() {
            return base.clone();
        }
        let explicit = self.linker_symbols();
        match base {
            ExportPolicy::Explicit(existing) => {
                let mut merged = existing.clone();
                for sym in explicit {
                    if !merged.iter().any(|s| s == &sym) {
                        merged.push(sym);
                    }
                }
                ExportPolicy::Explicit(merged)
            }
            other => {
                let _ = other;
                ExportPolicy::Explicit(explicit)
            }
        }
    }
}

impl ExportTableEntry {
    fn from_codegen(symbol_id: u32, entry: &ExportEntry) -> Self {
        Self {
            symbol_id,
            beskid_name: entry.beskid_name.clone(),
            exported_symbol: entry.exported_symbol.clone(),
            abi: entry.abi.clone(),
        }
    }
}
