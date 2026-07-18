use std::fs;
use std::path::PathBuf;
use std::time::Duration;

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
fn design_changes_are_blocked_during_active_runs() {
    let mut app = App {
        run_active: true,
        ..App::default()
    };
    for character in "/design".chars() {
        app.handle_key(key(KeyCode::Char(character)));
    }

    assert!(app.handle_key(key(KeyCode::Enter)).is_empty());
    assert_eq!(app.modal, ModalState::None);

    app.apply(AppEvent::DesignPickerOpened {
        entries: vec![("Acme".into(), PathBuf::from("/designs/acme"))],
    });
    assert_eq!(app.modal, ModalState::None);
}

#[test]
fn design_picker_selects_a_package() {
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

#[test]
fn import_events_update_app_state_without_entering_transcript() {
    let mut app = App::default();
    app.apply(AppEvent::ImportDesignStarted {
        source: PathBuf::from("/tmp/acme.pptx"),
    });
    assert!(app.run_active);
    assert_eq!(
        app.import_design_status,
        Some(ImportDesignStatus::Running(ImportProgress {
            source_name: "acme.pptx".into(),
            stage: ImportDesignStage::Reading,
            percent: None,
            animation_frame: 0,
        }))
    );

    app.apply(AppEvent::ImportDesignProgress {
        stage: ImportDesignStage::Building,
        percent: Some(140),
    });
    let Some(ImportDesignStatus::Running(progress)) = &app.import_design_status else {
        panic!("import was not running");
    };
    assert_eq!(progress.stage, ImportDesignStage::Building);
    assert_eq!(progress.percent, Some(100));
    assert!(app.transcript.is_empty());
}

#[test]
fn tick_animates_indeterminate_import_and_expires_confirmation() {
    let mut app = App::default();
    app.apply(AppEvent::ImportDesignStarted {
        source: PathBuf::from("acme.pptx"),
    });
    let tick = std::time::Instant::now() + Duration::from_millis(100);
    app.apply(AppEvent::Tick(tick));
    let Some(ImportDesignStatus::Running(progress)) = &app.import_design_status else {
        panic!("import was not running");
    };
    assert_eq!(progress.animation_frame, 1);

    app.apply(AppEvent::ImportDesignCompleted {
        design_name: "Acme".into(),
    });
    assert!(!app.run_active);
    let Some(ImportDesignStatus::Completed { expires_at, .. }) = &app.import_design_status else {
        panic!("completion was not shown");
    };
    let expires_at = *expires_at;
    app.apply(AppEvent::Tick(expires_at));
    assert_eq!(app.import_design_status, None);
}

#[test]
fn failed_import_persists_while_cancelled_import_expires() {
    let mut app = App::default();
    app.apply(AppEvent::ImportDesignFailed {
        error: "unsupported theme".into(),
    });
    app.apply(AppEvent::Tick(
        std::time::Instant::now() + Duration::from_secs(60),
    ));
    assert!(matches!(
        app.import_design_status,
        Some(ImportDesignStatus::Failed { .. })
    ));

    app.apply(AppEvent::ImportDesignCancelled);
    let Some(ImportDesignStatus::Cancelled { expires_at }) = &app.import_design_status else {
        panic!("cancellation was not shown");
    };
    let expires_at = *expires_at;
    app.apply(AppEvent::Tick(expires_at));
    assert_eq!(app.import_design_status, None);
}
