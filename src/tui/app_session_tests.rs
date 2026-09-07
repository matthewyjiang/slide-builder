use super::*;
use crate::tui::modal::session_picker::SessionPickerEntry;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn session_commands_request_picker_without_sending_a_prompt() {
    for command in ["/sessions", "/resume", " /RESUME "] {
        for run_active in [false, true] {
            let mut app = App {
                run_active,
                ..App::default()
            };
            app.input.set_text(command);
            assert_eq!(
                app.handle_key(key(KeyCode::Enter)),
                vec![if command == "/sessions" {
                    AppAction::OpenSessionManager
                } else {
                    AppAction::OpenSessionPicker
                }]
            );
            assert!(app.transcript.is_empty());
            assert!(app.input.text.is_empty());
            assert_eq!(app.run_active, run_active);
        }
    }
}

#[test]
fn session_picker_filters_selects_and_keeps_failures_visible_until_cancelled() {
    let entries = ["Research", "Launch"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| SessionPickerEntry {
            id: index.to_string(),
            name: name.into(),
            deck: format!("{name}.pptx"),
            model: "provider/model".into(),
            current: index == 0,
        })
        .collect();
    let mut app = App::default();
    app.input.set_text("draft stays here");
    app.apply(AppEvent::SessionPickerOpened { entries });
    app.handle_key(key(KeyCode::Char('l')));
    app.handle_key(key(KeyCode::Char('a')));
    assert_eq!(
        app.handle_key(key(KeyCode::Enter)),
        vec![AppAction::ResumeSession("1".into())]
    );
    app.apply(AppEvent::SessionResumeFailed("Deck is missing".into()));
    let ModalState::SessionPicker(state) = &app.modal else {
        panic!("picker closed before resume succeeded")
    };
    assert_eq!(state.error.as_deref(), Some("Deck is missing"));
    assert_eq!(state.filter, "la");
    assert!(app.handle_key(key(KeyCode::Esc)).is_empty());
    assert_eq!(app.modal, ModalState::None);
    assert_eq!(app.input.text, "draft stays here");
    assert!(app.transcript.is_empty());
}

#[test]
fn both_palette_entries_open_saved_sessions() {
    for command in [Command::Sessions, Command::ResumeSession] {
        let mut app = App::default();
        assert_eq!(
            app.run_command(command),
            vec![if command == Command::Sessions {
                AppAction::OpenSessionManager
            } else {
                AppAction::OpenSessionPicker
            }]
        );
    }
}

#[test]
fn manager_dispatches_changes_and_preserves_errors_and_filter() {
    let mut app = App::default();
    app.input.set_text("draft");
    let entry = SessionPickerEntry {
        id: "saved".into(),
        name: "Research".into(),
        deck: "deck.pptx".into(),
        model: "provider/model".into(),
        current: false,
    };
    app.apply(AppEvent::SessionManagerOpened {
        entries: vec![entry.clone()],
    });
    app.handle_key(key(KeyCode::Char('R')));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        app.handle_key(key(KeyCode::Enter)),
        vec![AppAction::ResumeSession("saved".into())]
    );
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        app.handle_key(key(KeyCode::Enter)),
        vec![AppAction::RenameSession {
            id: "saved".into(),
            name: "Research".into()
        }]
    );
    app.apply(AppEvent::SessionResumeFailed("Database is busy".into()));
    let ModalState::SessionPicker(state) = &app.modal else {
        panic!("manager closed")
    };
    assert!(matches!(
        state.view,
        crate::tui::modal::session_picker::SessionView::Rename { .. }
    ));
    assert_eq!(state.error.as_deref(), Some("Database is busy"));
    app.apply(AppEvent::SessionManagementSucceeded {
        entries: vec![entry],
    });
    let ModalState::SessionPicker(state) = &app.modal else {
        panic!("manager closed")
    };
    assert_eq!(state.filter, "R");
    assert_eq!(state.error, None);
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Down));
    assert_eq!(
        app.handle_key(key(KeyCode::Enter)),
        vec![AppAction::DeleteSession("saved".into())]
    );
    app.apply(AppEvent::SessionManagementSucceeded { entries: vec![] });
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.modal, ModalState::None);
    assert_eq!(app.input.text, "draft");
}

#[test]
fn session_manager_pastes_names_without_submitting_or_confirming_actions() {
    let mut app = App::default();
    app.apply(AppEvent::SessionManagerOpened {
        entries: vec![SessionPickerEntry {
            id: "saved".into(),
            name: "Research".into(),
            deck: "deck.pptx".into(),
            model: "provider/model".into(),
            current: false,
        }],
    });
    let paste = |text: &str| AppEvent::Input(crossterm::event::Event::Paste(text.into()));
    app.apply(paste("Research"));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.apply(paste("\nDelete")).is_empty());
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert!(app.apply(paste("Quarterly review\n")).is_empty());
    assert_eq!(
        app.handle_key(key(KeyCode::Enter)),
        vec![AppAction::RenameSession {
            id: "saved".into(),
            name: "Quarterly review".into(),
        }]
    );
}
