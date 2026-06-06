use super::super::services::JournalService;
use super::super::types::JournalEntry;

pub struct Journal {
    entries: Vec<JournalEntry>,
}

impl Default for Journal {
    fn default() -> Self {
        Self::new()
    }
}

impl Journal {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl JournalService for Journal {
    fn record(&mut self, entry: JournalEntry) {
        self.entries.push(entry);
    }
    fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }
}
