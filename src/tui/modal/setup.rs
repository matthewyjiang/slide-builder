use std::path::PathBuf;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetupState {
    pub decks_dir: Option<PathBuf>,
    pub browser_path: Option<PathBuf>,
    pub protocol_supported: bool,
    pub diagnostic: Option<String>,
}
