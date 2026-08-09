/// Activity data is intentionally storage-neutral; adapters supply stable sequence IDs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryActivityEntry {
    sequence: u64,
    occurred_at_unix_seconds: i64,
    action: String,
    message: String,
}

impl RegistryActivityEntry {
    pub fn new(
        sequence: u64,
        occurred_at_unix_seconds: i64,
        action: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self { sequence, occurred_at_unix_seconds, action: action.into(), message: message.into() }
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryActivityLog {
    entries: Vec<RegistryActivityEntry>,
    capacity: usize,
}

impl RegistryActivityLog {
    pub const LEGACY_CAPACITY: usize = 500;

    pub fn legacy_compatible() -> Self {
        Self::with_capacity(Self::LEGACY_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "registry activity capacity must be positive");
        Self { entries: Vec::new(), capacity }
    }

    pub fn append(&mut self, entry: RegistryActivityEntry) {
        self.entries.push(entry);
        self.entries.sort_by(|left, right| {
            right
                .occurred_at_unix_seconds
                .cmp(&left.occurred_at_unix_seconds)
                .then_with(|| right.sequence.cmp(&left.sequence))
        });
        self.entries.truncate(self.capacity);
    }

    pub fn entries(&self) -> &[RegistryActivityEntry] {
        &self.entries
    }

    pub fn recent(&self, take: usize) -> &[RegistryActivityEntry] {
        &self.entries[..take.min(self.capacity).min(self.entries.len())]
    }
}
