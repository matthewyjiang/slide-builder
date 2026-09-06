use super::*;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn configuration_round_trips() {
    let config = Config::default();
    assert_eq!(
        ConfigurationState::new(&config, vec![])
            .to_config()
            .unwrap(),
        config
    );
}

#[test]
fn bad_numeric_value_is_reported_without_closing() {
    let mut state = ConfigurationState::new(&Config::default(), vec![]);
    if let MenuValue::Text(value) = &mut state.menu.groups[2].items[2].value {
        *value = "nope".into();
    }
    let event = state.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(event, ConfigurationEvent::None);
    assert!(state.menu.status.is_some());
}

#[test]
fn scaled_obscura_configuration_saves() {
    let mut config = Config::default();
    config.preview.scale = 2;
    let mut state = ConfigurationState::new(&config, vec![]);
    assert_eq!(
        state.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        ConfigurationEvent::Save(Box::new(config))
    );
}

#[test]
fn oversized_obscura_capture_is_reported_before_save() {
    let mut config = Config::default();
    config.preview.width = 4096;
    config.preview.scale = 2;
    let mut state = ConfigurationState::new(&config, vec![]);
    assert_eq!(
        state.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        ConfigurationEvent::None
    );
    assert!(state
        .menu
        .status
        .as_ref()
        .unwrap()
        .contains("8192x4608 output pixels"));
    config.preview.enabled = false;
    let mut state = ConfigurationState::new(&config, vec![]);
    assert_eq!(
        state.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        ConfigurationEvent::Save(Box::new(config))
    );
}

#[test]
fn renderer_choice_and_paths_save_without_losing_inactive_engine_settings() {
    let mut config = Config::default();
    config.render.sandbox_path = "/opt/bwrap".into();
    config.render.browser_path = "/opt/chromium".into();
    let mut state = ConfigurationState::new(&config, vec![]);
    state.menu.selected = state
        .menu
        .groups
        .iter()
        .flat_map(|group| &group.items)
        .position(|item| item.id == "render_engine")
        .unwrap();
    state.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    config.render.engine = RenderEngine::Chromium;
    assert_eq!(
        state.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        ConfigurationEvent::Save(Box::new(config.clone()))
    );
    assert_eq!(
        ConfigurationState::new(&config, vec![])
            .to_config()
            .unwrap(),
        config
    );
}

#[test]
fn model_choice_lists_only_provided_models_plus_the_configured_one() {
    let config = Config {
        provider: "anthropic".into(),
        model: "claude-old".into(),
        ..Default::default()
    };
    let available = AvailableModel {
        provider: "openai".into(),
        model: "gpt-x".into(),
        display_name: "GPT X".into(),
        auth: "api-key".into(),
    };
    let mut state = ConfigurationState::new(&config, vec![available.clone()]);
    let MenuValue::Choice { options, selected } = &state.menu.groups[0].items[0].value else {
        panic!("model is not a choice")
    };
    assert_eq!(
        options,
        &vec!["openai/gpt-x".to_owned(), "anthropic/claude-old".to_owned()]
    );
    assert_eq!(*selected, 1);

    state.menu.selected = 0;
    state.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    let saved = state.to_config().unwrap();
    assert_eq!(
        (
            saved.provider.as_str(),
            saved.auth.as_str(),
            saved.model.as_str()
        ),
        ("openai", "api-key", "gpt-x")
    );
}
