use super::*;
use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, ModifyKind};
use tokio::sync::oneshot;

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

#[tokio::test]
async fn completed_render_resolves_waiting_tool_with_ordered_paths() {
    let (response, receiver) = oneshot::channel();
    let mut pending = vec![PendingRenderTool {
        generation: 3,
        response,
    }];
    let event = AppEvent::RenderDone {
        generation: 3,
        manifest: RenderManifest {
            slides: vec![
                SlideRender {
                    index: 1,
                    image_path: "/tmp/slide-2.png".into(),
                },
                SlideRender {
                    index: 0,
                    image_path: "/tmp/slide-1.png".into(),
                },
            ],
        },
    };

    complete_render_tools(&event, &mut pending);

    assert!(pending.is_empty());
    assert_eq!(
        receiver.await.unwrap().unwrap(),
        vec![
            PathBuf::from("/tmp/slide-1.png"),
            PathBuf::from("/tmp/slide-2.png")
        ]
    );
}

#[tokio::test]
async fn failed_render_returns_the_error_to_waiting_tool() {
    let (response, receiver) = oneshot::channel();
    let mut pending = vec![PendingRenderTool {
        generation: 4,
        response,
    }];

    complete_render_tools(
        &AppEvent::RenderFailed {
            generation: 4,
            error: "browser capture failed".into(),
        },
        &mut pending,
    );

    assert_eq!(
        receiver.await.unwrap(),
        Err("browser capture failed".into())
    );
}
