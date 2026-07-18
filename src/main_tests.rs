use super::*;
use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, ModifyKind};

fn event(kind: EventKind, path: &Path) -> notify::Event {
    notify::Event::new(kind).add_path(path.to_path_buf())
}

#[test]
fn deck_watcher_ignores_renderer_reads() {
    let deck = Path::new("/tmp/deck.pptx");
    let read = event(EventKind::Access(AccessKind::Close(AccessMode::Read)), deck);

    assert!(!deck_content_changed(&read, deck));
}

#[test]
fn deck_watcher_accepts_content_changes_only_for_active_deck() {
    let deck = Path::new("/tmp/deck.pptx");
    let other_deck = Path::new("/tmp/other.pptx");

    assert!(deck_content_changed(
        &event(
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            deck,
        ),
        deck,
    ));
    assert!(deck_content_changed(
        &event(EventKind::Create(CreateKind::File), deck),
        deck,
    ));
    assert!(!deck_content_changed(
        &event(
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            other_deck,
        ),
        deck,
    ));
}

#[test]
fn design_import_workflow_events_stay_out_of_agent_chat_events() {
    assert_eq!(
        import_workflow_app_event(DesignImportWorkflowEvent::Stage(
            DesignImportWorkflowStage::Analyzing,
        )),
        AppEvent::ImportDesignProgress {
            stage: ImportDesignStage::Analyzing,
            percent: None,
        }
    );
    assert_eq!(
        import_workflow_app_event(DesignImportWorkflowEvent::Completed {
            package_name: "Acme".into(),
            package_path: PathBuf::from("/designs/acme"),
        }),
        AppEvent::ImportDesignCompleted {
            design_name: "Acme".into(),
        }
    );
}
