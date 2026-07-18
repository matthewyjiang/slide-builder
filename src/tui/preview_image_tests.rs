use super::*;
use std::time::{Duration, Instant};

#[test]
fn configured_protocol_names_are_case_insensitive() {
    assert_eq!(configured_protocol_type("Kitty"), Some(ProtocolType::Kitty));
    assert_eq!(configured_protocol_type("sixel"), Some(ProtocolType::Sixel));
    assert_eq!(configured_protocol_type("auto"), None);
    assert_eq!(configured_protocol_type("unknown"), None);
}

#[test]
fn centers_fitted_image_inside_available_area() {
    let area = Rect::new(10, 4, 40, 20);
    assert_eq!(
        centered_image_area(area, Size::new(30, 12)),
        Rect::new(15, 8, 30, 12)
    );
}

#[test]
fn clamps_fitted_image_to_available_area() {
    let area = Rect::new(2, 2, 10, 6);
    assert_eq!(
        centered_image_area(area, Size::new(20, 18)),
        Rect::new(2, 2, 10, 6)
    );
}

#[test]
fn warms_terminal_ready_protocols_for_each_cell_size() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("slide.png");
    write_test_image(&path);
    let mut preview = PreviewImage::detect_with_budget("halfblocks", 64);
    preview.preload_deck(vec![path.clone()]);

    preview.warm_for_sizes(&path, &[Size::new(40, 20)]);
    wait_until_idle(&mut preview);
    preview.warm_for_sizes(&path, &[Size::new(80, 40)]);
    wait_until_idle(&mut preview);

    assert!(preview.protocols.contains_key(&CacheKey {
        path: path.clone(),
        cells: Size::new(40, 20),
    }));
    assert!(preview.protocols.contains_key(&CacheKey {
        path,
        cells: Size::new(80, 40),
    }));
    for (key, encoded) in &preview.protocols {
        let resize = Resize::Scale(None);
        let fitted = encoded.protocol.size_for(resize.clone(), key.cells);
        assert_eq!(encoded.protocol.needs_resize(&resize, fitted), None);
    }
}

#[test]
fn warms_all_slides_for_preview_and_presentation_when_they_fit() {
    let directory = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (1..=3)
        .map(|index| directory.path().join(format!("slide-{index}.png")))
        .collect();
    for path in &paths {
        write_test_image(path);
    }
    let sizes = [Size::new(40, 20), Size::new(80, 40)];
    let mut preview = PreviewImage::detect_with_budget("halfblocks", 96);
    preview.preload_deck(paths.clone());

    for _ in 0..4 {
        if preview.protocols.len() == paths.len() * sizes.len() {
            break;
        }
        preview.warm_for_sizes(&paths[0], &sizes);
        wait_until_idle(&mut preview);
    }

    assert_eq!(preview.protocols.len(), 6);
    assert_eq!(preview.decoded_bytes, 96);
}

#[test]
fn replacing_deck_discards_stale_worker_results() {
    let directory = tempfile::tempdir().unwrap();
    let old_path = directory.path().join("old.png");
    let current_path = directory.path().join("current.png");
    write_test_image(&old_path);
    write_test_image(&current_path);
    let mut preview = PreviewImage::detect_with_budget("halfblocks", 64);

    preview.preload_deck(vec![old_path.clone()]);
    preview.warm_for_sizes(&old_path, &[Size::new(40, 20)]);
    preview.preload_deck(vec![current_path.clone()]);
    preview.warm_for_sizes(&current_path, &[Size::new(40, 20)]);
    wait_until_idle(&mut preview);

    assert!(preview.protocols.keys().all(|key| key.path != old_path));
    assert!(preview.protocols.contains_key(&CacheKey {
        path: current_path,
        cells: Size::new(40, 20)
    }));
}

#[test]
fn failed_decode_is_reported_for_the_requested_size() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing.png");
    let cells = Size::new(40, 20);
    let mut preview = PreviewImage::detect_with_budget("halfblocks", 64);
    preview.preload_deck(vec![path.clone()]);

    preview.warm_for_sizes(&path, &[cells]);
    wait_until_idle(&mut preview);

    let error = preview.errors.get(&CacheKey { path, cells }).unwrap();
    assert!(error.contains("could not open image"));
}

#[test]
fn reactivating_a_rendered_kitty_slide_swaps_in_a_preencoded_protocol() {
    let directory = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (1..=2)
        .map(|index| directory.path().join(format!("slide-{index}.png")))
        .collect();
    for path in &paths {
        write_test_image(path);
    }
    let cells = Size::new(40, 20);
    let first_key = CacheKey {
        path: paths[0].clone(),
        cells,
    };
    let second_key = CacheKey {
        path: paths[1].clone(),
        cells,
    };
    let mut preview = PreviewImage::detect_with_budget("kitty", 64);
    preview.preload_deck(paths.clone());
    preview.warm_for_sizes(&paths[0], &[cells]);
    wait_until_idle(&mut preview);

    preview.activate(&first_key);
    assert!(preview.protocols.contains_key(&first_key));
    preview.protocols.get_mut(&first_key).unwrap().rendered = true;
    preview.activate(&second_key);
    preview.activate(&first_key);

    assert!(preview.protocols.contains_key(&first_key));
    assert!(preview.protocols[&first_key].standby.is_none());
    assert!(preview.pending.contains(&first_key));
    assert!(preview.protocols.contains_key(&second_key));

    wait_until_idle(&mut preview);
    assert!(preview.protocols.contains_key(&first_key));
    assert!(preview.protocols[&first_key].standby.is_some());

    preview.protocols.get_mut(&first_key).unwrap().rendered = true;
    preview.activate(&second_key);
    preview.activate(&first_key);
    assert!(preview.protocols.contains_key(&first_key));
    assert!(preview.pending.contains(&first_key));
}

#[test]
fn reactivating_a_halfblocks_slide_reuses_its_cached_protocol() {
    let directory = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (1..=2)
        .map(|index| directory.path().join(format!("slide-{index}.png")))
        .collect();
    for path in &paths {
        write_test_image(path);
    }
    let cells = Size::new(40, 20);
    let first_key = CacheKey {
        path: paths[0].clone(),
        cells,
    };
    let second_key = CacheKey {
        path: paths[1].clone(),
        cells,
    };
    let mut preview = PreviewImage::detect_with_budget("halfblocks", 32);
    preview.preload_deck(paths.clone());
    preview.warm_for_sizes(&paths[0], &[cells]);
    wait_until_idle(&mut preview);

    preview.activate(&first_key);
    preview.protocols.get_mut(&first_key).unwrap().rendered = true;
    preview.activate(&second_key);
    preview.activate(&first_key);

    assert!(preview.protocols.contains_key(&first_key));
    assert!(preview.attempted.contains(&first_key));
}

#[test]
fn terminal_protocol_cache_is_memory_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (1..=3)
        .map(|index| directory.path().join(format!("slide-{index}.png")))
        .collect();
    for path in &paths {
        write_test_image(path);
    }
    let mut preview = PreviewImage::detect_with_budget("halfblocks", 32);
    preview.preload_deck(paths.clone());
    preview.warm_for_sizes(&paths[0], &[Size::new(40, 20)]);
    wait_until_idle(&mut preview);

    assert_eq!(preview.protocols.len(), 2);
    assert_eq!(preview.decoded_bytes, 32);
}

fn wait_until_idle(preview: &mut PreviewImage) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !preview.pending.is_empty() && Instant::now() < deadline {
        preview.collect_completed();
        std::thread::sleep(Duration::from_millis(5));
    }
    preview.collect_completed();
    assert!(preview.pending.is_empty(), "image worker timed out");
}

fn write_test_image(path: &Path) {
    image::RgbaImage::from_pixel(2, 2, image::Rgba([12, 34, 56, 255]))
        .save(path)
        .unwrap();
}
