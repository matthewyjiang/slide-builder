use super::*;
use std::io::{Cursor, Read};

const SOURCE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8" width="8" height="8"><rect width="8" height="8" fill="#123456"/></svg>"##;

fn png() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(8, 8, image::Rgba([18, 52, 86, 255]));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

fn add_png(handler: &PptxHandler) {
    handler
        .add(
            "/slide[1]",
            "image",
            InsertPosition::Append,
            &HashMap::from([
                ("format".into(), "png".into()),
                ("payloadBase64".into(), STANDARD.encode(png())),
                ("x".into(), "2".into()),
                ("y".into(), "3".into()),
                ("width".into(), "4".into()),
                ("height".into(), "5".into()),
            ]),
            None,
        )
        .unwrap();
}

fn zip_text(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> String {
    let mut text = String::new();
    zip.by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn embeds_svg_with_png_fallback_and_preserves_existing_pictures() {
    for existing_pictures in [0, 2] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fallback.pptx");
        std::fs::write(&path, crate::agent::deck_engine::BLANK_DECK).unwrap();
        let handler = PptxHandler::open(path.to_str().unwrap(), true).unwrap();
        handler
            .add("/", "slide", InsertPosition::Append, &HashMap::new(), None)
            .unwrap();
        super::super::ensure_drawing_namespace(&handler, "/slide[1]").unwrap();
        for _ in 0..existing_pictures {
            add_png(&handler);
        }
        add_png(&handler);
        let before = handler
            .raw("ppt/slides/slide1.xml", RawOptions::default())
            .unwrap();
        let original = Document::parse(&before).unwrap();
        let pictures = original
            .descendants()
            .filter(|node| node.has_tag_name((PRESENTATION, "pic")))
            .map(|node| before[node.range()].to_owned())
            .collect::<Vec<_>>();
        let fallback = last_picture(&original).unwrap();
        let original_blip = picture_blip(fallback).unwrap();
        let png_embed = original_blip.attribute((RELATIONSHIP, "embed")).unwrap();
        let png_geometry = fallback
            .children()
            .find(|node| node.has_tag_name((PRESENTATION, "spPr")))
            .map(|node| before[node.range()].to_owned())
            .unwrap();

        let svg = crate::render::svg::validate_and_render(SOURCE).unwrap();
        attach(&handler, "/slide[1]", &svg).unwrap();
        handler.save().unwrap();
        drop(handler);
        let reopened = PptxHandler::open(path.to_str().unwrap(), false).unwrap();
        assert!(reopened.validate().unwrap().is_empty());
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let xml = zip_text(&mut zip, "ppt/slides/slide1.xml");
        let document = Document::parse(&xml).unwrap();
        let final_pictures = document
            .descendants()
            .filter(|node| node.has_tag_name((PRESENTATION, "pic")))
            .collect::<Vec<_>>();
        assert_eq!(final_pictures.len(), existing_pictures + 1);
        for (picture, original) in final_pictures.iter().zip(&pictures).take(existing_pictures) {
            assert_eq!(&xml[picture.range()], original);
        }
        let picture = last_picture(&document).unwrap();
        let blip = picture_blip(picture).unwrap();
        assert_eq!(blip.attribute((RELATIONSHIP, "embed")), Some(png_embed));
        assert!(xml[picture.range()].contains(&png_geometry));
        let svg = blip
            .descendants()
            .find(|node| node.has_tag_name((SVG, "svgBlip")))
            .unwrap();
        assert_eq!(svg.parent().unwrap().attribute("uri"), Some(SVG_EXTENSION));
        let svg_embed = svg.attribute((RELATIONSHIP, "embed")).unwrap();
        assert_ne!(png_embed, svg_embed);
        let rels = zip_text(&mut zip, "ppt/slides/_rels/slide1.xml.rels");
        require_image_relationship(&rels, png_embed, "png").unwrap();
        require_image_relationship(&rels, svg_embed, "svg").unwrap();
        let relationships = Document::parse(&rels).unwrap();
        for (embed, expected) in [(png_embed, png()), (svg_embed, SOURCE.as_bytes().to_vec())] {
            let target = relationships
                .descendants()
                .find(|node| node.attribute("Id") == Some(embed))
                .unwrap()
                .attribute("Target")
                .unwrap();
            let name = format!("ppt/{}", target.strip_prefix("../").unwrap());
            let mut actual = Vec::new();
            zip.by_name(&name)
                .unwrap()
                .read_to_end(&mut actual)
                .unwrap();
            assert_eq!(actual, expected);
        }
        let content_types = zip_text(&mut zip, "[Content_Types].xml");
        assert!(content_types.contains("image/png"));
        assert!(content_types.contains("image/svg+xml"));
    }
}

#[test]
fn appends_to_existing_blip_extension_list_without_rewriting_unknown_xml() {
    let xml = format!(
        r#"<p:sld xmlns:p="{PRESENTATION}" xmlns:a="{DRAWING}" xmlns:r="{RELATIONSHIP}"><p:cSld><p:spTree><p:pic keep='yes'><p:blipFill><a:blip r:embed="png" cstate='print'><a:extLst><a:ext uri="existing"><custom xmlns="urn:test" attr='&amp;'/></a:ext></a:extLst></a:blip></p:blipFill></p:pic><!--keep--><p:pic><p:blipFill><a:blip r:embed="svg"/></p:blipFill></p:pic></p:spTree></p:cSld></p:sld>"#
    );
    let rels = format!(
        r#"<Relationships xmlns="{PACKAGE_RELATIONSHIP}"><Relationship Id="png" Type="{RELATIONSHIP}/image" Target="../media/fallback.png"/><Relationship Id="svg" Type="{RELATIONSHIP}/image" Target="../media/source.svg"/></Relationships>"#
    );
    let document = Document::parse(&xml).unwrap();
    let original = document
        .descendants()
        .find(|node| node.has_tag_name((PRESENTATION, "pic")))
        .unwrap();
    let result = merge_picture(&xml, &rels, "png", &xml[original.range()]).unwrap();
    let updated = Document::parse(&result).unwrap();
    assert_eq!(
        updated
            .descendants()
            .filter(|node| node.has_tag_name((PRESENTATION, "pic")))
            .count(),
        1
    );
    assert_eq!(
        updated
            .descendants()
            .filter(|node| node.has_tag_name((DRAWING, "extLst")))
            .count(),
        1
    );
    assert!(result.contains("<p:pic keep='yes'>"));
    assert!(result.contains("<a:blip r:embed=\"png\" cstate='print'>"));
    assert!(result
        .contains("<a:ext uri=\"existing\"><custom xmlns=\"urn:test\" attr='&amp;'/></a:ext>"));
    assert!(result.contains("<!--keep-->"));
    assert!(merge_picture(&xml, &rels, "other", &xml[original.range()]).is_err());
    assert!(require_image_relationship(&rels, "svg", "png").is_err());
}
