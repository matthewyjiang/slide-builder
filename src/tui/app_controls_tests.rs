use super::*;

fn shortcut(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn sending_a_message_returns_the_conversation_to_latest() {
    let mut app = App {
        conversation_scroll_offset: 12,
        ..App::default()
    };
    app.input.text = "new request".into();
    app.input.cursor = app.input.text.len();

    app.handle_key(shortcut(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.conversation_scroll_offset, 0);
}

#[test]
fn action_palette_opens_from_common_terminal_key_variants() {
    for code in [KeyCode::Char('k'), KeyCode::Char('K')] {
        let mut app = App::default();
        assert!(app
            .handle_key(shortcut(code, KeyModifiers::CONTROL))
            .is_empty());
        assert!(matches!(app.modal, ModalState::CommandPalette(_)));
    }

    let mut app = App::default();
    app.handle_key(shortcut(KeyCode::F(2), KeyModifiers::NONE));
    assert!(matches!(app.modal, ModalState::CommandPalette(_)));
}

#[test]
fn slash_command_tab_completion_uses_the_filtered_match() {
    let mut app = App::default();
    app.input.text = "/re".into();
    app.input.cursor = app.input.text.len();

    app.handle_key(shortcut(KeyCode::Tab, KeyModifiers::NONE));

    assert_eq!(app.input.text, "/render");
    assert_eq!(app.input.cursor, app.input.text.len());
    assert_eq!(
        app.handle_key(shortcut(KeyCode::Enter, KeyModifiers::NONE)),
        vec![AppAction::RequestRender]
    );
    assert!(app.input.text.is_empty());
}

#[test]
fn slash_command_selection_can_be_navigated_and_run() {
    let mut app = App::default();
    app.input.text = "/".into();
    app.input.cursor = app.input.text.len();

    // /actions is first and /open is second.
    app.handle_key(shortcut(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(
        app.handle_key(shortcut(KeyCode::Enter, KeyModifiers::NONE)),
        vec![AppAction::OpenDeckPicker]
    );
    assert!(matches!(app.modal, ModalState::DeckPicker(_)));
}

#[test]
fn escape_dismisses_slash_suggestions_until_the_input_changes() {
    let mut app = App::default();
    app.input.text = "/con".into();
    app.input.cursor = app.input.text.len();
    assert_eq!(app.input.slash_suggestions().len(), 1);

    app.handle_key(shortcut(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.input.slash_suggestions().is_empty());

    app.handle_key(shortcut(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(!app.input.slash_suggestions().is_empty());
}

#[test]
fn ctrl_c_clears_the_composer_before_quitting() {
    let mut app = App::default();
    app.input.text = "unfinished prompt".into();
    app.input.cursor = app.input.text.len();

    assert!(app
        .handle_key(shortcut(KeyCode::Char('c'), KeyModifiers::CONTROL))
        .is_empty());
    assert!(app.input.text.is_empty());
    assert_eq!(app.input.cursor, 0);
    assert!(!app.should_quit);

    assert_eq!(
        app.handle_key(shortcut(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        vec![AppAction::Quit]
    );
    assert!(app.should_quit);
}

#[test]
fn slide_prefix_refreshes_and_presents_without_leaving_prompt_input() {
    let mut app = App::default();
    app.preview.slides.push(SlideItem {
        title: "Opening".into(),
        image_path: None,
    });

    app.handle_key(shortcut(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert_eq!(
        app.handle_key(shortcut(KeyCode::Char('r'), KeyModifiers::NONE)),
        vec![AppAction::RequestRender]
    );

    app.handle_key(shortcut(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert!(app
        .handle_key(shortcut(KeyCode::Char('f'), KeyModifiers::NONE))
        .is_empty());
    assert!(app.fullscreen);
    assert!(app.input.text.is_empty());
}

#[test]
fn action_palette_has_a_prompt_command_fallback() {
    let mut app = App::default();
    app.input.text = "/actions".into();
    app.input.cursor = app.input.text.len();
    app.handle_key(shortcut(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.modal, ModalState::CommandPalette(_)));
    assert!(app.input.text.is_empty());
}
