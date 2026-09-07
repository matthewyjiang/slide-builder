use super::*;
use ratatui::{backend::TestBackend, Terminal};

fn entries() -> Vec<SessionPickerEntry> {
    (0..30)
        .map(|index| SessionPickerEntry {
            id: format!("id-{index}"),
            name: format!("Session {index:02}"),
            deck: format!("deck-{index}.pptx"),
            model: "provider/model".into(),
            current: index == 0,
        })
        .collect()
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn capture(state: &SessionPickerState, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render(frame, state)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn filtering_matches_metadata_and_resets_navigation() {
    let mut state = SessionPickerState::new(entries());
    state.handle_key(key(KeyCode::End));
    for c in "DECK-7".chars() {
        state.handle_key(key(KeyCode::Char(c)));
    }
    assert_eq!(state.selected, 0);
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        SessionPickerEvent::Selected("id-7".into())
    );
    state.handle_key(key(KeyCode::Char('x')));
    assert_eq!(
        state.handle_key(key(KeyCode::Enter)),
        SessionPickerEvent::None
    );
    state.handle_key(key(KeyCode::Backspace));
    assert_eq!(state.visible().len(), 1);
    assert_eq!(
        state.handle_key(key(KeyCode::Esc)),
        SessionPickerEvent::Cancel
    );
}

#[test]
fn narrow_and_wide_lists_scroll_selected_row_into_view() {
    for (width, height) in [(32, 10), (80, 24)] {
        let mut state = SessionPickerState::new(entries());
        let initial = capture(&state, width, height);
        assert!(initial.contains("current · Session 00"), "{initial}");
        state.handle_key(key(KeyCode::End));
        let last = capture(&state, width, height);
        assert!(last.contains("› Session 29"), "{last}");
        assert!(!last.contains("Session 00"), "{last}");
        state.handle_key(key(KeyCode::Up));
        let previous = capture(&state, width, height);
        assert!(previous.contains("› Session 28"), "{previous}");
        state.handle_key(key(KeyCode::Home));
        assert!(capture(&state, width, height).contains("› current · Session 00"));
    }
}

#[test]
fn empty_filtered_and_inline_error_states_render() {
    let empty = SessionPickerState::new(vec![]);
    assert!(capture(&empty, 40, 10).contains("No saved sessions yet."));
    let mut state = SessionPickerState::new(entries());
    state.filter = "missing".into();
    assert!(capture(&state, 40, 10).contains("No sessions match this filter."));
    state.filter.clear();
    state.error = Some("Deck is missing. Choose another session.".into());
    let error = capture(&state, 32, 10);
    assert!(error.contains("Notice: Deck is missing."), "{error}");
    assert!(error.contains("Choose another session."), "{error}");
    for (width, height) in [(1, 1), (12, 4), (20, 6)] {
        capture(&state, width, height);
    }
}
