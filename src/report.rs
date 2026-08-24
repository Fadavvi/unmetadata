/// Summary of what was stripped from a single file.
#[derive(Debug, Default, Clone)]
pub struct StripReport {
    pub items: Vec<String>,
}

impl StripReport {
    pub fn add(&mut self, item: impl Into<String>) {
        self.items.push(item.into());
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn merge(&mut self, other: StripReport) {
        self.items.extend(other.items);
    }
}
