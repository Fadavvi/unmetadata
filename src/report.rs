/// One removed container (tag block, chunk, box, ...) plus the names of the
/// individual tags/fields it contained. Field entries are *names only* -
/// their values never appear in the report.
#[derive(Debug, Clone)]
pub struct StripItem {
    pub summary: String,
    pub fields: Vec<String>,
}

/// Summary of what was stripped from a single file.
#[derive(Debug, Default, Clone)]
pub struct StripReport {
    pub items: Vec<StripItem>,
}

impl StripReport {
    /// Record a removed container without enumerating its fields.
    pub fn add(&mut self, summary: impl Into<String>) {
        self.items.push(StripItem {
            summary: summary.into(),
            fields: Vec::new(),
        });
    }

    /// Record a removed container together with the names of the tags/fields
    /// it contained (e.g. "TITLE", "ARTIST").
    pub fn add_fields(&mut self, summary: impl Into<String>, fields: Vec<String>) {
        self.items.push(StripItem {
            summary: summary.into(),
            fields,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
