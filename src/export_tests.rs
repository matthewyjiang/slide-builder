use super::*;

#[test]
fn arguments_and_default_destination() {
    assert_eq!(parse_arguments("PDF").unwrap(), None);
    assert_eq!(
        parse_arguments("pdf output/my slides.pdf").unwrap(),
        Some("output/my slides.pdf".into())
    );
    assert!(parse_arguments("")
        .unwrap_err()
        .to_string()
        .contains("Usage:"));
    assert!(parse_arguments("pptx")
        .unwrap_err()
        .to_string()
        .contains("Unsupported export format"));
    assert!(parse_arguments("pdf original.pptx").is_err());
    assert_eq!(
        destination(Path::new("/decks/talk.pptx"), None),
        PathBuf::from("/decks/talk.pdf")
    );
}

#[test]
fn publication_never_overwrites_and_cleans_temporary_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("talk.pdf");
    publish(&path, b"original").unwrap();
    assert!(publish(&path, b"replacement").is_err());
    assert!(check_destination(&path)
        .unwrap_err()
        .to_string()
        .contains("already exists"));
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    assert!(publish(&directory.path().join("missing/talk.pdf"), b"pdf").is_err());
}

#[test]
fn pdf_pages_are_full_bleed_in_slide_order() {
    let directory = tempfile::tempdir().unwrap();
    let paths = [[255, 0, 0], [0, 0, 255]]
        .into_iter()
        .enumerate()
        .map(|(index, color)| {
            let path = directory.path().join(format!("{index}.png"));
            image::RgbImage::from_pixel(30, 50, image::Rgb(color))
                .save(&path)
                .unwrap();
            path
        })
        .collect::<Vec<_>>();
    let bytes = pdf::assemble(&paths, 300.0, 500.0).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(text.matches("/MediaBox [0 0 300 500]").count(), 2);
    assert_eq!(text.matches("300 0 0 500 0 0 cm").count(), 2);
    assert!(text.contains("/Count 2"));
    assert!(pdf::assemble(&[], 300.0, 500.0).is_err());
    assert!(pdf::assemble(&paths, f32::NAN, 500.0).is_err());
}
