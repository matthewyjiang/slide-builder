use std::path::PathBuf;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesignPickerState {
    pub entries: Vec<(String, PathBuf)>,
    /// Index into `entries` for the active design, if listed.
    pub current: Option<usize>,
    pub selected: usize,
    pub filter: String,
}

impl DesignPickerState {
    /// Opens with the active design highlighted so reopening the picker keeps the prior choice.
    pub fn new(entries: Vec<(String, PathBuf)>, current: Option<usize>) -> Self {
        debug_assert!(current.is_none_or(|index| index < entries.len()));
        Self {
            selected: current.unwrap_or(0),
            entries,
            current,
            filter: String::new(),
        }
    }
}
