use super::*;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use slide_builder::tui::{AppAction, ApprovalRequest};

fn apply(status: &mut Status, app: &mut App, event: AppEvent) -> (HerdrState, &'static str) {
    status.observe(&event);
    app.apply(event);
    status.current(app)
}

#[test]
fn prompt_approval_and_completion_follow_app_input_ownership() {
    let mut app = App::default();
    let mut status = Status::default();
    assert_eq!(
        status.current(&app),
        (HerdrState::Idle, "Ready for a prompt")
    );
    app.input.text = "Create a slide".into();
    let actions = app.apply(AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    ))));
    assert!(matches!(
        actions.as_slice(),
        [AppAction::SendMessage { .. }]
    ));
    assert_eq!(status.current(&app), (HerdrState::Working, "Editing deck"));
    assert_eq!(
        apply(
            &mut status,
            &mut app,
            AppEvent::Approval(ApprovalRequest {
                id: "approval".into(),
                title: "Approve tool".into(),
                detail: "private arguments".into(),
                allow_for_session: false,
            })
        ),
        (HerdrState::Blocked, "Waiting for tool approval")
    );
    let actions = app.apply(AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    ))));
    assert!(matches!(
        actions.as_slice(),
        [AppAction::RespondApproval { .. }]
    ));
    assert_eq!(status.current(&app), (HerdrState::Working, "Editing deck"));
    assert_eq!(
        apply(
            &mut status,
            &mut app,
            AppEvent::Run(AgentEvent::RunFinished)
        ),
        (HerdrState::Idle, "Deck editing finished")
    );
    assert_eq!(
        apply(
            &mut status,
            &mut app,
            AppEvent::RenderStarted { generation: 1 }
        ),
        (HerdrState::Idle, "Deck editing finished")
    );
}

#[test]
fn failures_and_cancellation_return_to_ready_without_leaking_details() {
    for (event, message) in [
        (
            AgentEvent::RunFailed("private provider response".into()),
            "Deck editing failed; see conversation",
        ),
        (AgentEvent::RunCancelled, "Deck editing cancelled"),
    ] {
        let mut app = App {
            run_active: true,
            ..App::default()
        };
        let mut status = Status::default();
        assert_eq!(
            apply(&mut status, &mut app, AppEvent::Run(event)),
            (HerdrState::Idle, message)
        );
        app.run_active = true;
        assert_eq!(status.current(&app), (HerdrState::Working, "Editing deck"));
    }
}

#[test]
fn export_import_and_dialogs_take_precedence_over_idle() {
    let mut app = App {
        export_active: true,
        ..App::default()
    };
    let mut status = Status::default();
    assert_eq!(status.current(&app), (HerdrState::Working, "Exporting PDF"));
    assert_eq!(
        apply(
            &mut status,
            &mut app,
            AppEvent::Run(AgentEvent::RunFinished)
        ),
        (HerdrState::Working, "Exporting PDF")
    );
    assert_eq!(
        apply(
            &mut status,
            &mut app,
            AppEvent::ExportFinished(Ok("deck.pdf".into()))
        ),
        (HerdrState::Idle, "PDF exported")
    );
    assert_eq!(
        apply(
            &mut status,
            &mut app,
            AppEvent::ImportDesignStarted {
                source: "design.pptx".into()
            }
        ),
        (HerdrState::Working, "Importing design")
    );
    assert_eq!(
        apply(&mut status, &mut app, AppEvent::ImportDesignCancelled),
        (HerdrState::Idle, "Design import cancelled")
    );
    app.modal = ModalState::Help;
    assert_eq!(status.current(&app).0, HerdrState::Blocked);
    app.apply(AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::NONE,
    ))));
    assert_eq!(status.current(&app).0, HerdrState::Idle);
    app.fullscreen = true;
    assert_eq!(status.current(&app).0, HerdrState::Blocked);
}

#[test]
fn host_graphics_choose_kitty_with_cell_metrics_or_halfblocks() {
    assert!(preview_picker(HerdrGraphicsCapability::NotHerdr).is_none());
    let fallback = preview_picker(HerdrGraphicsCapability::Unpaintable).unwrap();
    assert_eq!(fallback.protocol_type(), ProtocolType::Halfblocks);
    let kitty = preview_picker(HerdrGraphicsCapability::Paintable {
        width: 9,
        height: 18,
    })
    .unwrap();
    assert_eq!(kitty.protocol_type(), ProtocolType::Kitty);
    assert_eq!((kitty.font_size().width, kitty.font_size().height), (9, 18));
}
