use super::*;
use pretty_assertions::assert_eq;
use ratatui::{backend::TestBackend, Terminal};

fn deliver(cache: &mut ConversationImages, result: Completed) -> bool {
    let (sender, receiver) = mpsc::sync_channel(1);
    sender.send(result).unwrap();
    let original = std::mem::replace(&mut cache.completed, receiver);
    let changed = cache.poll();
    cache.completed = original;
    changed
}

fn fixture_image(path: &Path) {
    // The halfblock picker's 10x20 pixel cell size makes this a 12x6-cell image.
    let pixels = image::RgbImage::from_fn(120, 120, |_, y| {
        if y < 60 {
            image::Rgb([255, 0, 0])
        } else {
            image::Rgb([0, 0, 255])
        }
    });
    pixels.save(path).unwrap();
}

#[test]
fn image_completion_between_paint_and_mouse_up_uses_painted_copy_and_selection() {
    use crate::tui::{App, AppAction, AppEvent, Message, Role, TranscriptItem};
    use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    for drag in [false, true] {
        let mut app = App::default();
        app.mouse.viewport = Rect::new(0, 0, 140, 60);
        app.transcript = vec![TranscriptItem::Message(Message {
            role: Role::Assistant,
            text: "![plot](image.png)\n```rust\nlet value = 1;\n```".into(),
            complete: true,
        })];
        let (sender, receiver) = mpsc::sync_channel(1);
        app.conversation_images.borrow_mut().completed = receiver;
        let mut terminal = Terminal::new(TestBackend::new(140, 60)).unwrap();
        terminal
            .draw(|frame| crate::tui::render(frame, &app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let (x, y) = (0..60)
            .find_map(|y| {
                (0..136).find_map(|x| {
                    ((0..4)
                        .map(|i| buffer[(x + i, y)].symbol())
                        .collect::<String>()
                        == "COPY")
                        .then_some((x, y))
                })
            })
            .expect("fallback frame has a copy header");
        let mouse = |kind, column| {
            AppEvent::Input(Event::Mouse(MouseEvent {
                kind,
                column,
                row: y,
                modifiers: KeyModifiers::NONE,
            }))
        };
        app.apply(mouse(MouseEventKind::Down(MouseButton::Left), x));
        let width = crate::tui::layout::regions(app.mouse.viewport, &app)
            .chat
            .width
            - 2;
        let available = Size::new(width, 60);
        let image = encode_image(
            &Picker::halfblocks(),
            image::DynamicImage::new_rgb8(120, 240),
            available,
        )
        .unwrap();
        sender
            .send(Completed {
                generation: 0,
                key: Key {
                    path: PathBuf::from("image.png"),
                    available,
                },
                result: Ok(image),
            })
            .unwrap();
        let end = if drag { x + 3 } else { x };
        if drag {
            app.apply(mouse(MouseEventKind::Drag(MouseButton::Left), end));
        }
        let actions = app.apply(mouse(MouseEventKind::Up(MouseButton::Left), end));
        assert_eq!(
            actions,
            vec![AppAction::CopyText(
                if drag { "COPY" } else { "let value = 1;" }.into()
            )]
        );
        assert!(
            app.conversation_images.borrow().ready.is_empty(),
            "input must not poll completions"
        );
        app.mouse.selection = None;
        app.mouse.toast = None;
        terminal
            .draw(|frame| crate::tui::render(frame, &app))
            .unwrap();
        assert!(app
            .conversation_images
            .borrow()
            .cached_image("image.png", available)
            .is_some());
        assert_ne!(terminal.backend().buffer()[(x, y)].symbol(), "C");
    }
}

#[test]
fn retained_media_layout_tracks_ready_error_and_clear_without_pinning_images() {
    use crate::tui::{conversation_markdown::MessageCache, Message, Role};
    use std::rc::Rc;

    crate::tui::syntax::warm_syntax_set();
    let available = Size::new(12, 10);
    let key = Key {
        path: PathBuf::from("image.png"),
        available,
    };
    let image = encode_image(
        &Picker::halfblocks(),
        image::DynamicImage::new_rgb8(120, 120),
        available,
    )
    .unwrap();
    let protocol = Arc::downgrade(&image.protocol);
    let mut images = ConversationImages::default();
    images.ready.insert(key.clone(), image);
    let message = Message {
        role: Role::Assistant,
        text: "![plot](image.png)\n```rust\nlet x = 1;\n```".into(),
        complete: true,
    };
    let mut messages = MessageCache::default();
    messages.prepare(14, 1);
    let first = messages.message_with_images(0, &message, &images, available);
    assert_eq!(first.code_blocks[0].top_line, 7);
    let second = messages.message_with_images(0, &message, &images, available);
    assert!(Rc::ptr_eq(&first, &second));
    // A different image's completion cannot invalidate this message's layout.
    images.failed.insert(
        Key {
            path: PathBuf::from("other.png"),
            available,
        },
        "missing".into(),
    );
    assert!(Rc::ptr_eq(
        &first,
        &messages.message_with_images(0, &message, &images, available)
    ));
    images.clear();
    assert!(
        protocol.upgrade().is_none(),
        "text snapshots cannot pin evicted protocols"
    );
    let fallback = messages.message_with_images(0, &message, &images, available);
    assert_eq!(fallback.code_blocks[0].top_line, 2);
    assert_eq!(
        first.code_blocks[0].top_line, 7,
        "old painted anchors stay immutable"
    );
    images.failed.insert(key, "missing".into());
    let failed = messages.message_with_images(0, &message, &images, available);
    assert!(failed.code_blocks[0].top_line > fallback.code_blocks[0].top_line);
    assert!(Rc::ptr_eq(
        &failed,
        &messages.message_with_images(0, &message, &images, available)
    ));
}

#[test]
fn resolves_local_references_and_rejects_remote_or_control_paths() {
    let cwd = Path::new("/workspace");
    let home = Some(Path::new("/home/test"));
    for (source, expected) in [
        ("images/a.png", Some("/workspace/images/a.png")),
        ("/tmp/a.png", Some("/tmp/a.png")),
        ("~/a.png", Some("/home/test/a.png")),
        ("file:///tmp/a%20b.png", Some("/tmp/a b.png")),
        ("file://localhost/tmp/a.png", Some("/tmp/a.png")),
        ("https://example.org/a.png", None),
        ("file://example.org/tmp/a.png", None),
        ("data:image/png;base64,abc", None),
        ("//example.org/a.png", None),
        ("file:///tmp/%00.png", None),
        ("file:///tmp/%ZZ.png", None),
        ("a\n.png", None),
    ] {
        assert_eq!(
            local_path(source, cwd, home),
            expected.map(PathBuf::from),
            "{source:?}"
        );
    }
}

#[test]
fn background_load_is_cached_and_clear_ignores_late_results() {
    let temp = tempfile::tempdir().unwrap();
    fixture_image(&temp.path().join("image.png"));
    let size = Size::new(12, 6);
    let mut cache = ConversationImages::new(temp.path().to_owned(), Some(Picker::halfblocks()));
    assert!(cache.image("image.png", size).is_none());
    // Waiting is only in the test. Production draws always use try_recv/try_send.
    let done = cache
        .completed
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let image = done.result.as_ref().unwrap().clone();
    assert!(deliver(&mut cache, done));
    let cached = cache.image("image.png", size).unwrap();
    assert!(Arc::ptr_eq(&image.protocol, &cached.protocol));

    let old_generation = cache.generation;
    cache.clear();
    assert!(cache.ready.is_empty());
    assert_ne!(old_generation, cache.generation);
    assert!(cache.image("image.png", size).is_none());
    cache.clear();
    // A queued load may complete, but cannot repopulate a cleared session.
    let late = cache
        .completed
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert_ne!(late.generation, cache.generation);
    assert!(!deliver(&mut cache, late));
    assert!(cache.ready.is_empty());
}

#[test]
fn queue_full_retries_eventually_load_each_image_without_duplicate_work() {
    let temp = tempfile::tempdir().unwrap();
    let sources = ["a.png", "b.png", "c.png", "d.png"];
    for source in sources {
        fixture_image(&temp.path().join(source));
    }
    let size = Size::new(12, 6);
    let mut cache = ConversationImages::new(temp.path().to_owned(), Some(Picker::halfblocks()));
    // More images than the worker + queue can accept simultaneously.
    while cache.ready.len() < sources.len() {
        for source in sources {
            cache.image(source, size);
        }
        if cache.ready.len() == sources.len() {
            break;
        }
        let done = cache
            .completed
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(deliver(&mut cache, done));
    }
    assert!(cache.pending.is_empty());
    assert!(cache.failed.is_empty());
}

#[test]
fn reserves_only_ready_source_rows_and_crops_without_resizing() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("image.png");
    fixture_image(&path);
    let image = load_image(
        &Picker::halfblocks(),
        &Key {
            path: path.clone(),
            available: Size::new(12, 6),
        },
    )
    .unwrap();
    let mut cache = ConversationImages::new(temp.path().to_owned(), None);
    cache.ready.insert(
        Key {
            path,
            available: Size::new(12, 6),
        },
        image.clone(),
    );
    let rendered = crate::tui::markdown::render_markdown(
        "![missing](missing.png)\n![ready](image.png)\n```\nafter\n```",
        12,
    );
    let rendered = crate::tui::conversation_media::MediaLayout::default().render(
        std::rc::Rc::new(rendered),
        &cache,
        Size::new(12, 6),
        /*padding*/ 0,
    );
    assert!(rendered
        .lines
        .first()
        .unwrap()
        .to_string()
        .contains("missing"));
    assert_eq!(rendered.code_blocks[0].top_line, 7);
    assert!(rendered.lines.last().unwrap().to_string().contains("after"));

    let mut full = Terminal::new(TestBackend::new(12, 6)).unwrap();
    let full_placement = ImagePlacement { image, rows: 0..6 };
    full.draw(|frame| full_placement.render(frame, frame.area(), 0))
        .unwrap();
    let mut clipped = Terminal::new(TestBackend::new(12, 3)).unwrap();
    clipped
        .draw(|frame| full_placement.render(frame, frame.area(), 3))
        .unwrap();
    for y in 0..3 {
        for x in 0..12 {
            assert_eq!(
                clipped.backend().buffer()[(x, y)],
                full.backend().buffer()[(x, y + 3)]
            );
        }
    }
}

#[test]
fn image_budget_diagnostics_wrap_and_preserve_following_copy_targets() {
    let available = Size::new(20, 10);
    let mut cache = ConversationImages::new(PathBuf::from("/workspace"), None);
    cache.failed.insert(
        Key {
            path: PathBuf::from("/workspace/large.png"),
            available,
        },
        "decoded image budget 256 bytes exceeded: requested 512 bytes".into(),
    );
    let rendered =
        crate::tui::markdown::render_markdown("![large](large.png)\n```rust\nlet x = 1;\n```", 20);
    let rendered = crate::tui::conversation_media::MediaLayout::default().render(
        std::rc::Rc::new(rendered),
        &cache,
        available,
        /*padding*/ 0,
    );
    let copy = &rendered.code_blocks[0];
    let diagnostics = rendered.lines[1..copy.top_line]
        .iter()
        .map(ToString::to_string)
        .collect::<String>();
    assert!(diagnostics.contains("requested 512 bytes"));
    assert!(rendered.lines[copy.top_line].to_string().contains("COPY"));
    assert_eq!(copy.text, "let x = 1;");
}

#[test]
fn rejects_non_files_and_corrupt_images() {
    let temp = tempfile::tempdir().unwrap();
    let picker = Picker::halfblocks();
    let mut key = Key {
        path: temp.path().to_owned(),
        available: Size::new(12, 6),
    };
    assert!(load_image(&picker, &key)
        .unwrap_err()
        .contains("regular local file"));
    key.path = temp.path().join("invalid.png");
    std::fs::write(&key.path, b"not an image").unwrap();
    assert!(load_image(&picker, &key).is_err());
}

#[test]
fn accepted_render_completion_reloads_overwritten_image_paths() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("image.png");
    fixture_image(&path);
    let available = Size::new(12, 6);
    let mut app = crate::tui::App::default();
    *app.conversation_images.borrow_mut() =
        ConversationImages::new(temp.path().to_owned(), Some(Picker::halfblocks()));
    let original = {
        let mut cache = app.conversation_images.borrow_mut();
        cache.request_visible("image.png", available);
        let done = cache
            .completed
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        deliver(&mut cache, done);
        cache.cached_image("image.png", available).unwrap()
    };
    image::RgbImage::new(20, 20).save(path).unwrap();
    app.apply(crate::tui::AppEvent::RenderDone {
        generation: 1,
        manifest: Default::default(),
    });
    let mut cache = app.conversation_images.borrow_mut();
    assert!(cache.ready.is_empty());
    cache.request_visible("image.png", available);
    let done = cache
        .completed
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    deliver(&mut cache, done);
    let updated = cache.cached_image("image.png", available).unwrap();
    assert_ne!(original.size(), updated.size());
}

#[test]
fn evicted_images_retry_when_visible_without_speculative_cache_churn() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("image.png");
    fixture_image(&path);
    let available = Size::new(12, 6);
    let key = Key { path, available };
    let mut image = load_image(&Picker::halfblocks(), &key).unwrap();
    // Simulate a cache-filling decoded source without allocating hundreds of MiB in the test.
    image.decoded_bytes = IMAGE_BYTES;
    let mut cache = ConversationImages::new(temp.path().to_owned(), Some(Picker::halfblocks()));
    let generation = cache.generation;
    assert!(deliver(
        &mut cache,
        Completed {
            generation,
            key: key.clone(),
            result: Ok(image.clone()),
        }
    ));
    let second = Key {
        path: temp.path().join("second.png"),
        available,
    };
    assert!(deliver(
        &mut cache,
        Completed {
            generation,
            key: second,
            result: Ok(image),
        }
    ));
    assert!(cache.image("image.png", available).is_none());
    assert!(cache.pending.is_empty());
    assert!(cache.load_error("image.png", available).is_none());
    cache.request_visible("image.png", available);
    assert!(cache.pending.contains(&key));
    let done = cache
        .completed
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert!(deliver(&mut cache, done));
    assert!(cache.image("image.png", available).is_some());
}

#[test]
fn kitty_reactivation_retransmits_and_refreshes_without_rereading_source() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("image.png");
    fixture_image(&path);
    let size = Size::new(12, 6);
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ProtocolType::Kitty);
    let mut cache = ConversationImages::new(temp.path().to_owned(), Some(picker));
    cache.image("image.png", size);
    let done = cache
        .completed
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    deliver(&mut cache, done);
    let placement = ImagePlacement {
        image: cache.image("image.png", size).unwrap(),
        rows: 0..6,
    };
    std::fs::remove_file(path).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(12, 6)).unwrap();
    let draw = |terminal: &mut Terminal<TestBackend>| {
        terminal
            .draw(|frame| placement.render(frame, frame.area(), 0))
            .unwrap();
        terminal.backend().buffer()[(0, 0)]
            .symbol()
            .contains("\x1b_G")
    };
    assert!(draw(&mut terminal));
    cache.end_frame();
    assert!(
        !draw(&mut terminal),
        "continuous visibility must not retransmit"
    );
    cache.end_frame();
    cache.end_frame(); // One frame without a placement, as when another view covers chat.
    assert!(draw(&mut terminal), "reactivation must retransmit");
    cache.end_frame();
    cache.end_frame();
    assert!(!draw(&mut terminal), "standby is still pending until poll");
    cache.end_frame();
    let refresh = cache
        .completed
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert!(
        refresh.result.is_ok(),
        "refresh uses retained resized pixels, not the removed file"
    );
    deliver(&mut cache, refresh);
    assert!(
        draw(&mut terminal),
        "late replacement must retransmit while visible"
    );
    cache.end_frame();
    cache.end_frame();
    assert!(
        draw(&mut terminal),
        "replenished standby supports another reactivation"
    );
}

#[test]
fn cache_charges_resized_pixels_and_worker_panics_clear_pending() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("image.png");
    fixture_image(&path);
    let key = Key {
        path,
        available: Size::new(2, 1),
    };
    let picker = Picker::halfblocks();
    let image = load_image(&picker, &key).unwrap();
    assert_eq!(image.decoded_bytes, 20 * 20 * 4);
    let mut cache = ConversationImages::new(temp.path().to_owned(), None);
    cache.pending.insert(key.clone());
    let result = catch_image_panic(|| panic!("injected codec panic"));
    let generation = cache.generation;
    deliver(
        &mut cache,
        Completed {
            generation,
            key: key.clone(),
            result,
        },
    );
    assert!(cache.pending.is_empty());
    assert!(cache
        .load_error("image.png", key.available)
        .unwrap()
        .contains("injected codec panic"));
    assert!(catch_image_panic(|| load_image(&picker, &key)).is_ok());
}
