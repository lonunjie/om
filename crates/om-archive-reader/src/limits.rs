//! Caller-controlled resource ceilings. Defaults meet §3.4's minimum capacities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_entries: u64,
    pub max_entry_size: u64,
    pub max_total_size: u64,
    pub max_manifest_size: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 1_000_000,
            max_entry_size: 64 * 1024 * 1024 * 1024,
            max_total_size: 1024 * 1024 * 1024 * 1024,
            max_manifest_size: 64 * 1024 * 1024,
        }
    }
}
