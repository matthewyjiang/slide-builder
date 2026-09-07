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
                vec![AppAction::OpenSessionPicker]
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
        assert_eq!(app.run_command(command), vec![AppAction::OpenSessionPicker]);
    }
}
