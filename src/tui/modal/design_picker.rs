use std::path::PathBuf;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesignPickerState {
    pub entries: Vec<(String, PathBuf)>,
    pub selected: usize,
    pub filter: String,
}
