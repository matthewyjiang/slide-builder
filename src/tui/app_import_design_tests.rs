use std::fs;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::tempdir;

use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn import_design_slash_command_requests_picker_start_state() {
    let mut app = App::default();
    for character in "/import-design".chars() {
        app.handle_key(key(KeyCode::Char(character)));
    }

    assert_eq!(
        app.handle_key(key(KeyCode::Enter)),
        vec![AppAction::OpenImportDesignPicker]
    );
    assert!(app.input.text.is_empty());
}

#[test]
fn picker_event_uses_supplied_directory_and_escape_cancels() {
    let root = tempdir().unwrap();
    let mut app = App::default();

    assert!(app
        .apply(AppEvent::ImportDesignPickerOpened {
            start_directory: root.path().to_path_buf(),
        })
        .is_empty());
    let ModalState::ImportDesignPicker(state) = &app.modal else {
        panic!("import picker was not opened");
    };
    assert_eq!(state.current_directory, root.path());

    assert!(app.handle_key(key(KeyCode::Esc)).is_empty());
    assert_eq!(app.modal, ModalState::None);
}

#[test]
fn selecting_powerpoint_emits_import_action_and_closes_picker() {
    let root = tempdir().unwrap();
    let design = root.path().join("source.pptx");
    fs::write(&design, []).unwrap();
    let mut app = App::default();
    app.apply(AppEvent::ImportDesignPickerOpened {
        start_directory: root.path().to_path_buf(),
    });
    let ModalState::ImportDesignPicker(state) = &mut app.modal else {
        panic!("import picker was not opened");
    };
    state.selected = state
        .entries
        .iter()
        .position(|entry| entry.path == design)
        .unwrap();

    assert_eq!(
        app.handle_key(key(KeyCode::Enter)),
        vec![AppAction::ImportDesign(design)]
    );
    assert_eq!(app.modal, ModalState::None);
}

#[test]
fn managed_design_picker_selects_a_package() {
    let path = std::path::PathBuf::from("/data/design-packages/acme");
    let mut app = App::default();
    app.apply(AppEvent::DesignPickerOpened {
        entries: vec![("Acme".into(), path.clone())],
    });

    assert_eq!(
        app.handle_key(key(KeyCode::Enter)),
        vec![AppAction::SelectDesign(path)]
    );
    assert_eq!(app.modal, ModalState::None);
}
