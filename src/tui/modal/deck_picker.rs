use std::path::PathBuf;

use ratatui::Frame;

use super::filesystem_picker::{self, FileSystemPickerState};

/// Builds the picker used by `/open`: a filesystem browser that lists `.pptx`
/// files and accepts a typed name for a deck that does not exist yet.
pub fn deck_picker(start_directory: PathBuf) -> FileSystemPickerState {
    FileSystemPickerState::new(start_directory).allowing_new_files()
}

pub fn render(frame: &mut Frame<'_>, state: &FileSystemPickerState) {
    filesystem_picker::render_titled(frame, state, " Open deck ");
}
