//! One losslessly compressed slide image per full-bleed PDF page.
use anyhow::{bail, Context, Result};
use miniz_oxide::deflate::{compress_to_vec_zlib, CompressionLevel};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};
use std::path::PathBuf;

pub(super) fn assemble(images: &[PathBuf], width: f32, height: f32) -> Result<Vec<u8>> {
    if images.is_empty()
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        bail!("PDF needs at least one slide and finite positive page dimensions");
    }
    let mut pdf = Pdf::new();
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let mut next = Ref::new(3);
    let pages = images.iter().map(|_| next.bump()).collect::<Vec<_>>();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(pages.iter().copied())
        .count(i32::try_from(pages.len())?);
    for (path, page_id) in images.iter().zip(pages) {
        let image_id = next.bump();
        let content_id = next.bump();
        let name = Name(b"Slide");
        let mut page = pdf.page(page_id);
        page.parent(tree)
            .media_box(Rect::new(0.0, 0.0, width, height))
            .contents(content_id);
        page.resources().x_objects().pair(name, image_id);
        page.finish();

        let rgba = image::open(path)
            .with_context(|| format!("decode slide image {}", path.display()))?
            .to_rgba8();
        // Flatten alpha onto the same white background as the capture document.
        let rgb = rgba
            .pixels()
            .flat_map(|pixel| {
                let alpha = u32::from(pixel[3]);
                [0, 1, 2].map(|channel| {
                    ((u32::from(pixel[channel]) * alpha + 255 * (255 - alpha) + 127) / 255) as u8
                })
            })
            .collect::<Vec<_>>();
        let compressed = compress_to_vec_zlib(&rgb, CompressionLevel::DefaultLevel as u8);
        let mut image = pdf.image_xobject(image_id, &compressed);
        image.filter(Filter::FlateDecode);
        image
            .width(i32::try_from(rgba.width())?)
            .height(i32::try_from(rgba.height())?)
            .bits_per_component(8);
        image.color_space().device_rgb();
        image.finish();
        let mut content = Content::new();
        content
            .save_state()
            .transform([width, 0.0, 0.0, height, 0.0, 0.0])
            .x_object(name)
            .restore_state();
        pdf.stream(content_id, &content.finish());
    }
    Ok(pdf.finish())
}
