use super::*;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn configuration_round_trips() {
    let config = Config::default();
    assert_eq!(
        ConfigurationState::new(&config).to_config().unwrap(),
        config
    );
}

#[test]
fn bad_numeric_value_is_reported_without_closing() {
    let mut state = ConfigurationState::new(&Config::default());
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
    let mut state = ConfigurationState::new(&config);
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
    let mut state = ConfigurationState::new(&config);
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
    let mut state = ConfigurationState::new(&config);
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
    let mut state = ConfigurationState::new(&config);
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
        ConfigurationState::new(&config).to_config().unwrap(),
        config
    );
}
