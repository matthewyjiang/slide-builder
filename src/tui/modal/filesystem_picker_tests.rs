use std::fs;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::tempdir;

use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn typing_filters_rendered_decks_and_enter_opens_the_highlighted_match() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("Quarterly Review.pptx"), []).unwrap();
    fs::write(root.path().join("notes.pptx"), []).unwrap();
    let mut state = FileSystemPickerState::new(root.path().to_path_buf());
    for character in "qrv".chars() {
        state.handle_key(key(KeyCode::Char(character)));
    }
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| render_titled(frame, &state, " Open deck "))
        .unwrap();
    let screen = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(screen.contains("Quarterly Review.pptx"));
    assert!(!screen.contains("notes.pptx"));
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        FileSystemPickerEvent::Selected(root.path().join("Quarterly Review.pptx"))
    );
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
fn filtered_navigation_backspace_and_paste_keep_selection_in_sync() {
    let root = tempdir().unwrap();
    for name in ["alpha.pptx", "alpine.pptx", "beta.pptx"] {
        fs::write(root.path().join(name), []).unwrap();
    }
    let mut state = FileSystemPickerState::new(root.path().to_path_buf());
    state.handle_key(key(KeyCode::End));
    state.paste("alp");
    assert_eq!(state.selected, 0);
    state.handle_key(key(KeyCode::Down));
    state.handle_key(key(KeyCode::Down));
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        FileSystemPickerEvent::Selected(root.path().join("alpine.pptx"))
    );
    state.handle_key(key(KeyCode::Char('z')));
    assert!(state.filtered_entries().is_empty());
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        FileSystemPickerEvent::None
    );
    state.handle_key(key(KeyCode::Backspace));
    assert_eq!(state.selected, 0);
    assert!(state.error.is_none());
    assert_eq!(state.filtered_entries().len(), 2);
    for _ in 0..3 {
        state.handle_key(key(KeyCode::Backspace));
    }
    assert_eq!(state.filtered_entries().len(), state.entries.len());
}

#[test]
fn fuzzy_folder_selection_and_pasted_absolute_path_work() {
    let root = tempdir().unwrap();
    let folder = root.path().join("Project Decks");
    fs::create_dir(&folder).unwrap();
    let deck = folder.join("review.pptx");
    fs::write(&deck, []).unwrap();
    let mut state = FileSystemPickerState::new(root.path().to_path_buf());
    state.paste("pdk");
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        FileSystemPickerEvent::None
    );
    assert_eq!(state.current_directory, folder);
    assert!(state.path_input.is_empty());
    state.paste(&deck.to_string_lossy());
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        FileSystemPickerEvent::Selected(deck)
    );
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

#[test]
fn new_pptx_name_is_accepted_when_creating_is_allowed() {
    let root = tempdir().unwrap();
    let mut state = FileSystemPickerState::new(root.path().to_path_buf()).allowing_new_files();
    state.paste("fresh.pptx");

    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        FileSystemPickerEvent::Selected(root.path().join("fresh.pptx"))
    );
}

#[test]
fn new_non_pptx_name_is_rejected_even_when_creating_is_allowed() {
    let root = tempdir().unwrap();
    let mut state = FileSystemPickerState::new(root.path().to_path_buf()).allowing_new_files();
    state.paste("notes.txt");

    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        FileSystemPickerEvent::None
    );
    assert!(state.error.is_some());
}
