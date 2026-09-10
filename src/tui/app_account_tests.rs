use super::*;

#[test]
fn model_with_another_connection_is_not_marked_current() {
    let config = Config {
        provider: "xai".into(),
        auth: "xai-api-key".into(),
        model: "grok-4".into(),
        ..Config::default()
    };
    let mut model = AvailableModel::from_config(&config);
    assert!(model.matches_config(&config));
    model.auth = "xai-oauth".into();
    assert!(!model.matches_config(&config));
}

#[test]
fn account_commands_dispatch_without_sending_a_prompt() {
    for (text, action) in [
        ("/login", AppAction::Login),
        (" /LOGOUT ", AppAction::Logout),
    ] {
        for run_active in [false, true] {
            let mut app = App {
                run_active,
                ..App::default()
            };
            app.input.set_text(text);
            assert_eq!(
                app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                vec![action.clone()]
            );
            assert!(app.input.text.is_empty());
            assert!(app.transcript.is_empty());
            assert_eq!(app.run_active, run_active);
        }
    }
}

#[test]
fn account_suggestions_and_palette_dispatch_the_same_actions() {
    let suggestions = matching_slash_commands("/log");
    assert_eq!(
        suggestions
            .iter()
            .map(|command| command.name)
            .collect::<Vec<_>>(),
        vec!["/login", "/logout"]
    );
    for (command, action) in [
        (Command::Login, AppAction::Login),
        (Command::Logout, AppAction::Logout),
    ] {
        assert_eq!(App::default().run_command(command), vec![action]);
    }
}
