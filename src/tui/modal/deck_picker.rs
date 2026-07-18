use std::path::PathBuf;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeckPickerState {
    pub entries: Vec<PathBuf>,
    pub selected: usize,
    pub filter: String,
}
impl DeckPickerState {
    pub fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1).min(self.entries.len() - 1);
        }
    }
    pub fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}
