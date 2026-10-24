//! Recovery candidate model and constructors.

/// Atomic text edit applied at a byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairKind {
    InsertStatic { text: &'static str },
    InsertDynamic { text: String },
    Delete { len: usize },
    Replace { len: usize, text: String },
}

/// One recovery attempt: a single repair plus human-readable reason and try order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairCandidate {
    pub position: usize,
    pub kind: RepairKind,
    pub reason: &'static str,
    /// Lower values are tried first.
    pub priority: u8,
}

impl RepairCandidate {
    pub fn insert(position: usize, text: &'static str, reason: &'static str, priority: u8) -> Self {
        Self { position, kind: RepairKind::InsertStatic { text }, reason, priority }
    }

    pub fn insert_text(position: usize, text: String, reason: &'static str, priority: u8) -> Self {
        Self { position, kind: RepairKind::InsertDynamic { text }, reason, priority }
    }

    pub fn delete(position: usize, len: usize, reason: &'static str, priority: u8) -> Self {
        Self { position, kind: RepairKind::Delete { len }, reason, priority }
    }

    pub fn replace(position: usize, len: usize, text: &str, reason: &'static str, priority: u8) -> Self {
        Self { position, kind: RepairKind::Replace { len, text: text.to_string() }, reason, priority }
    }
}

impl RepairCandidate {
    /// The repair as the parse diagnostic sees it: an insertion, or something else.
    pub(crate) fn as_engine_repair(&self) -> crate::parsing::diagnose::EngineRepair<'_> {
        use crate::parsing::diagnose::EngineRepair;
        match &self.kind {
            RepairKind::InsertStatic { text } => EngineRepair::Insert { position: self.position, text },
            RepairKind::InsertDynamic { text } => EngineRepair::Insert { position: self.position, text },
            RepairKind::Delete { .. } | RepairKind::Replace { .. } => EngineRepair::Other,
        }
    }
}
