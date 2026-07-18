use std::path::PathBuf;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TemplatePickerState {
    pub entries: Vec<PathBuf>,
    pub selected: usize,
}
