//! Keep a raster fallback while attaching the Office SVG picture extension.
use crate::render::svg::ValidatedSvg;
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use handler_common::{output_format::RawOptions, DocumentHandler, InsertPosition};
use pptx_handler::PptxHandler;
use resvg::usvg::roxmltree::{Document, Node};
use std::collections::HashMap;

const PRESENTATION: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const RELATIONSHIP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_RELATIONSHIP: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const SVG: &str = "http://schemas.microsoft.com/office/drawing/2016/SVG/main";
const SVG_EXTENSION: &str = "{96DAC541-7B7A-43D3-8B79-37D633B846F1}";

/// Attach validated SVG to the PNG picture just appended to a physical slide.
/// The caller must use a transaction: registering the SVG changes package media
/// and relationships before the temporary picture is removed.
pub(super) fn attach(handler: &PptxHandler, parent: &str, svg: &ValidatedSvg) -> Result<()> {
    let index = parent
        .strip_prefix("/slide[")
        .and_then(|value| value.strip_suffix(']'))
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|index| *index > 0)
        .ok_or_else(|| {
            anyhow!("SVG attachment requires a physical /slide[N] parent, got `{parent}`")
        })?;
    let part = format!("ppt/slides/slide{index}.xml");
    let rels_part = format!("ppt/slides/_rels/slide{index}.xml.rels");
    let before = handler.raw(&part, RawOptions::default())?;
    let document = Document::parse(&before).context("parse PNG fallback slide")?;
    let picture = last_picture(&document)?;
    let png_embed = picture_blip(picture)?
        .attribute((RELATIONSHIP, "embed"))
        .ok_or_else(|| anyhow!("PNG fallback picture has no embedded image"))?
        .to_owned();
    let original_picture = before[picture.range()].to_owned();
    let rels = handler.raw(&rels_part, RawOptions::default())?;
    require_image_relationship(&rels, &png_embed, "png")?;

    // The handler owns package writes and content types. Register SVG through its
    // normal image adder, then retain its relationship without the extra picture.
    handler.add(
        parent,
        "image",
        InsertPosition::Append,
        &HashMap::from([
            ("format".into(), "svg".into()),
            (
                "payloadBase64".into(),
                STANDARD.encode(svg.source().as_bytes()),
            ),
        ]),
        None,
    )?;
    let xml = handler.raw(&part, RawOptions::default())?;
    let rels = handler.raw(&rels_part, RawOptions::default())?;
    let updated = merge_picture(&xml, &rels, &png_embed, &original_picture)?;
    super::replace_slide_document(handler, &part, &updated)
}

fn last_picture<'a>(document: &'a Document<'a>) -> Result<Node<'a, 'a>> {
    let tree = document
        .root_element()
        .children()
        .find(|node| node.has_tag_name((PRESENTATION, "cSld")))
        .and_then(|slide| {
            slide
                .children()
                .find(|node| node.has_tag_name((PRESENTATION, "spTree")))
        })
        .ok_or_else(|| anyhow!("SVG attachment slide has no shape tree"))?;
    tree.children()
        .rfind(Node::is_element)
        .filter(|node| node.has_tag_name((PRESENTATION, "pic")))
        .ok_or_else(|| anyhow!("SVG attachment expected the last slide element to be a picture"))
}

fn picture_blip<'a>(picture: Node<'a, 'a>) -> Result<Node<'a, 'a>> {
    picture
        .children()
        .find(|node| node.has_tag_name((PRESENTATION, "blipFill")))
        .and_then(|fill| {
            fill.children()
                .find(|node| node.has_tag_name((DRAWING, "blip")))
        })
        .ok_or_else(|| anyhow!("SVG attachment picture has no DrawingML blip"))
}

fn require_image_relationship(rels: &str, embed: &str, format: &str) -> Result<()> {
    let document = Document::parse(rels).context("parse image relationships")?;
    let mut matches = document.root_element().children().filter(|node| {
        node.has_tag_name((PACKAGE_RELATIONSHIP, "Relationship"))
            && node.attribute("Id") == Some(embed)
    });
    let relationship = matches
        .next()
        .ok_or_else(|| anyhow!("SVG attachment cannot resolve relationship `{embed}`"))?;
    if matches.next().is_some()
        || relationship.attribute("Type") != Some(&format!("{RELATIONSHIP}/image"))
        || relationship
            .attribute("TargetMode")
            .is_some_and(|mode| mode != "Internal")
        || !relationship
            .attribute("Target")
            .is_some_and(|target| target.to_ascii_lowercase().ends_with(&format!(".{format}")))
    {
        bail!("SVG attachment expected `{embed}` to reference an internal {format} image");
    }
    Ok(())
}

fn merge_picture(xml: &str, rels: &str, png_embed: &str, original: &str) -> Result<String> {
    let document = Document::parse(xml).context("parse slide after SVG registration")?;
    let temporary = last_picture(&document)?;
    let fallback = temporary
        .prev_siblings()
        .skip(1)
        .find(Node::is_element)
        .filter(|node| node.has_tag_name((PRESENTATION, "pic")))
        .ok_or_else(|| anyhow!("SVG attachment expected adjacent PNG and SVG pictures"))?;
    if &xml[fallback.range()] != original {
        bail!("SVG attachment PNG fallback changed during SVG registration");
    }
    let blip = picture_blip(fallback)?;
    if blip.attribute((RELATIONSHIP, "embed")) != Some(png_embed) {
        bail!("SVG attachment PNG fallback relationship changed");
    }
    let svg_embed = picture_blip(temporary)?
        .attribute((RELATIONSHIP, "embed"))
        .ok_or_else(|| anyhow!("temporary SVG picture has no embedded image"))?;
    require_image_relationship(rels, png_embed, "png")?;
    require_image_relationship(rels, svg_embed, "svg")?;
    if blip.descendants().any(|node| {
        node.has_tag_name((SVG, "svgBlip"))
            || (node.has_tag_name((DRAWING, "ext")) && node.attribute("uri") == Some(SVG_EXTENSION))
    }) {
        bail!("PNG fallback already has an SVG extension");
    }
    let extension = format!(
        "<a:ext xmlns:a=\"{DRAWING}\" uri=\"{SVG_EXTENSION}\"><asvg:svgBlip xmlns:asvg=\"{SVG}\" xmlns:r=\"{RELATIONSHIP}\" r:embed=\"{}\"/></a:ext>",
        quick_xml::escape::escape(svg_embed)
    );
    let lists = blip
        .children()
        .filter(|node| node.has_tag_name((DRAWING, "extLst")))
        .collect::<Vec<_>>();
    let (target, child) = match lists.as_slice() {
        [] => (
            blip,
            format!("<a:extLst xmlns:a=\"{DRAWING}\">{extension}</a:extLst>"),
        ),
        [list] => (*list, extension),
        _ => bail!("PNG fallback has multiple extension lists"),
    };
    let replacement = append_child(xml, target, &child)?;
    let mut updated = xml.to_owned();
    // Apply edits backwards so byte ranges remain valid. Everything outside
    // these two ranges, including picture geometry and unknown XML, is retained.
    updated.replace_range(temporary.range(), "");
    updated.replace_range(target.range(), &replacement);
    Document::parse(&updated).context("validate SVG fallback slide XML")?;
    Ok(updated)
}

fn append_child(xml: &str, node: Node<'_, '_>, child: &str) -> Result<String> {
    let fragment = &xml[node.range()];
    if let Some(open) = fragment.strip_suffix("/>") {
        let name = open
            .strip_prefix('<')
            .and_then(|open| open.split_whitespace().next())
            .ok_or_else(|| anyhow!("SVG attachment cannot read XML element name"))?;
        return Ok(format!("{open}>{child}</{name}>"));
    }
    let end = fragment
        .rfind("</")
        .ok_or_else(|| anyhow!("SVG attachment cannot find XML closing tag"))?;
    Ok(format!("{}{child}{}", &fragment[..end], &fragment[end..]))
}

#[cfg(test)]
#[path = "svg_picture_tests.rs"]
mod tests;
