use super::*;
use crossterm::event::KeyModifiers;

fn model(provider: &str, id: &str, name: &str) -> AvailableModel {
    AvailableModel {
        provider: provider.into(),
        model: id.into(),
        display_name: name.into(),
        auth: "api-key".into(),
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn picker_starts_on_the_current_model_and_selects_with_enter() {
    let models = vec![
        model("anthropic", "claude-a", "Claude A"),
        model("openai", "gpt-b", "GPT B"),
    ];
    let mut state = ModelPickerState::new(models.clone(), Some(1));
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        ModelPickerEvent::Selected(models[1].clone())
    );
}

#[test]
fn filter_narrows_the_list_and_selection_indexes_the_filtered_view() {
    let models = vec![
        model("anthropic", "claude-a", "Claude A"),
        model("openai", "gpt-b", "GPT B"),
        model("openai", "gpt-c", "GPT C"),
    ];
    let mut state = ModelPickerState::new(models.clone(), Some(0));
    for c in "gpt".chars() {
        state.handle_key(key(KeyCode::Char(c)));
    }
    assert_eq!(state.visible().len(), 2);
    state.handle_key(key(KeyCode::Down));
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        ModelPickerEvent::Selected(models[2].clone())
    );
}

#[test]
fn enter_on_an_empty_filter_result_does_nothing() {
    let mut state = ModelPickerState::new(vec![model("openai", "gpt-b", "GPT B")], None);
    state.handle_key(key(KeyCode::Char('z')));
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        ModelPickerEvent::None
    );
    assert_eq!(
        state.handle_key(key(KeyCode::Esc)),
        ModelPickerEvent::Cancel
    );
}
