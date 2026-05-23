//! Export metadata collected from [`beskid_codegen::CodegenArtifact::exports`].

use beskid_codegen::CodegenArtifact;
use beskid_codegen::ExportEntry;

use crate::api::ExportPolicy;

/// Linker-visible export row derived from codegen export metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportTableEntry {
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
                .map(ExportTableEntry::from_codegen)
                .collect(),
        }
    }

    pub fn entries(&self) -> &[ExportTableEntry] {
        &self.entries
    }

    pub fn linker_symbols(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.exported_symbol.clone())
            .collect()
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
    fn from_codegen(entry: &ExportEntry) -> Self {
        Self {
            beskid_name: entry.beskid_name.clone(),
            exported_symbol: entry.exported_symbol.clone(),
            abi: entry.abi.clone(),
        }
    }
}
