use super::*;

fn submit(app: &mut App, text: &str) -> Vec<AppAction> {
    app.input.set_text(text);
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
}

#[test]
fn export_command_validates_without_sending_an_agent_message() {
    let mut app = App::default();
    for text in ["/export", "/export svg", "/export pdf deck.pptx"] {
        assert!(submit(&mut app, text).is_empty());
        assert!(!app.run_active);
        assert!(!app.export_active);
        assert!(app.input.text.is_empty());
    }
    assert_eq!(
        submit(&mut app, "/export pdf"),
        vec![AppAction::ExportPdf(None)]
    );
    assert!(app.export_active);
    assert!(submit(&mut app, "/export pdf").is_empty());
    app.apply(AppEvent::ExportFinished(Ok(crate::export::ExportReport {
        path: "deck.pdf".into(),
        resolution_notice: Some("Export resolution reduced to fit renderer limits.".into()),
    })));
    assert!(!app.export_active);
    assert!(
        matches!(app.transcript.last(), Some(TranscriptItem::Message(Message { text, .. })) if text.contains("PDF exported to deck.pdf"))
    );
    assert!(
        matches!(app.transcript.last(), Some(TranscriptItem::Message(Message { text, .. })) if text.contains("Export resolution reduced"))
    );
    assert_eq!(
        submit(&mut app, "/export pdf my slides.pdf"),
        vec![AppAction::ExportPdf(Some("my slides.pdf".into()))]
    );
    app.apply(AppEvent::ExportFinished(Err("renderer unavailable".into())));
    assert!(!app.export_active);
    assert!(
        matches!(app.transcript.last(), Some(TranscriptItem::Message(Message { text, .. })) if text.contains("PDF export failed: renderer unavailable"))
    );
    app.run_active = true;
    assert!(submit(&mut app, "/export pdf").is_empty());
    assert!(!app.export_active);
}
