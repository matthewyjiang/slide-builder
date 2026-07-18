use super::{App, AppAction, AppEvent, PreviewStatus, SlideItem};

#[test]
fn agent_render_request_uses_the_normal_render_action() {
    let mut app = App::default();
    assert_eq!(
        app.apply(AppEvent::AgentRenderRequested),
        vec![AppAction::RequestRender]
    );
}

#[test]
fn agent_slide_selection_is_one_based_and_clamped() {
    let mut app = App::default();
    app.preview.status = PreviewStatus::Ready { generation: 1 };
    app.preview.slides = (1..=3)
        .map(|index| SlideItem {
            title: format!("Slide {index}"),
            image_path: None,
        })
        .collect();

    app.apply(AppEvent::AgentSetActiveSlide(2));
    assert_eq!(app.preview.active, 1);

    app.apply(AppEvent::AgentSetActiveSlide(99));
    assert_eq!(app.preview.active, 2);
}
