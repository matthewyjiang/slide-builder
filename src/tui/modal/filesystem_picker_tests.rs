use std::fs;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::tempdir;

use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn picker_lists_directories_and_powerpoint_files_only() {
    let root = tempdir().unwrap();
    fs::create_dir(root.path().join("folder")).unwrap();
    fs::write(root.path().join("theme.pptx"), []).unwrap();
    fs::write(root.path().join("UPPER.PPTX"), []).unwrap();
    fs::write(root.path().join("notes.txt"), []).unwrap();

    let state = FileSystemPickerState::new(root.path().to_path_buf());
    let names = state
        .entries
        .iter()
        .filter_map(|entry| entry.path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(names.contains(&"folder".into()));
    assert!(names.contains(&"theme.pptx".into()));
    assert!(names.contains(&"UPPER.PPTX".into()));
    assert!(!names.contains(&"notes.txt".into()));
}

#[test]
fn enter_opens_a_directory_then_selects_a_pptx() {
    let root = tempdir().unwrap();
    let folder = root.path().join("folder");
    fs::create_dir(&folder).unwrap();
    let design = folder.join("theme.pptx");
    fs::write(&design, []).unwrap();

    let mut state = FileSystemPickerState::new(root.path().to_path_buf());
    state.selected = state
        .entries
        .iter()
        .position(|entry| entry.path == folder)
        .unwrap();
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        FileSystemPickerEvent::None
    );
    assert_eq!(state.current_directory, folder);

    state.selected = state
        .entries
        .iter()
        .position(|entry| entry.path == design)
        .unwrap();
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        FileSystemPickerEvent::Selected(design)
    );
}

#[test]
fn typed_relative_path_can_select_a_pptx() {
    let root = tempdir().unwrap();
    let design = root.path().join("theme.pptx");
    fs::write(&design, []).unwrap();
    let mut state = FileSystemPickerState::new(root.path().to_path_buf());

    state.paste("theme.pptx");

    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        FileSystemPickerEvent::Selected(design)
    );
}

#[test]
fn invalid_typed_path_stays_open_with_an_error() {
    let root = tempdir().unwrap();
    let mut state = FileSystemPickerState::new(root.path().to_path_buf());
    state.paste("missing.pptx");

    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        FileSystemPickerEvent::None
    );
    assert!(state.error.is_some());
    assert_eq!(state.path_input, "missing.pptx");
}
