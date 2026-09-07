//! Validate a deliberately small, static SVG dialect before rasterizing in memory.

use anyhow::{ensure, Context, Result};
use quick_xml::{events::Event, Reader};
use resvg::{tiny_skia, usvg};
use usvg::roxmltree;

use crate::agent::deck_engine::MAX_MEDIA_BYTES;

const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
// Measured on x86_64 Linux, Rust 1.92 debug tests with resvg/usvg 0.47:
// transformed groups + shape opacity roundtrip at depth 128 on a 2 MiB
// worker stack, but overflow at 192 and 256. Depth 64 also passes on 1 MiB,
// leaving stack headroom on normal 2 MiB workers. Root depth is zero.
const MAX_SVG_DEPTH: usize = 64;

#[derive(Debug)]
pub(crate) struct ValidatedSvg {
    source: String,
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub aspect_ratio: f64,
    pub warnings: Vec<String>,
}

impl ValidatedSvg {
    pub(crate) fn source(&self) -> &str {
        &self.source
    }
}

/// No resource resolution, fonts, browser, or filesystem access is involved.
/// References, filters, masks, images and reuse are excluded, so input cannot
/// expand through recursive references or allocate filter-sized intermediate images.
/// Group opacity and dashed strokes are excluded too: deeply nested compositing
/// layers and tiny dash lengths can amplify otherwise small input dramatically.
pub(crate) fn validate_and_render(source: &str) -> Result<ValidatedSvg> {
    ensure!(
        source.len() as u64 <= MAX_MEDIA_BYTES,
        "SVG input byte budget exceeded: limit {MAX_MEDIA_BYTES}, asked {}",
        source.len()
    );
    preflight_xml(source)?;
    let document =
        roxmltree::Document::parse(source).context("invalid SVG XML; DTDs are forbidden")?;
    let root = document.root_element();
    ensure!(root.tag_name().name() == "svg", "document root must be svg");
    let namespace = root.tag_name().namespace();
    ensure!(
        namespace == Some(SVG_NAMESPACE),
        "SVG root requires the http://www.w3.org/2000/svg namespace"
    );
    for node in document.descendants() {
        ensure!(!node.is_pi(), "SVG processing instructions are forbidden");
        if node.is_text() {
            ensure!(
                node.text().unwrap_or_default().trim().is_empty(),
                "SVG text content is unsupported"
            );
        }
        if !node.is_element() {
            continue;
        }
        ensure!(
            node.tag_name().namespace() == namespace,
            "unsupported or mixed SVG namespace"
        );
        for declaration in node.namespaces() {
            ensure!(
                declaration.uri() == SVG_NAMESPACE,
                "unsupported namespace declaration: {}",
                declaration.uri()
            );
        }
        validate_element(node, root)?;
    }
    let view_box = numbers(
        root.attribute("viewBox")
            .context("SVG requires an explicit viewBox")?,
    )?;
    ensure!(
        view_box.len() == 4 && view_box[2] > 0.0 && view_box[3] > 0.0,
        "viewBox must contain four finite numbers with positive width and height"
    );
    finite(view_box[0] + view_box[2]).context("viewBox right edge is outside renderer range")?;
    finite(view_box[1] + view_box[3]).context("viewBox bottom edge is outside renderer range")?;
    ensure!(
        root.has_attribute("width") == root.has_attribute("height"),
        "SVG width and height must be provided together or both omitted"
    );
    let document_width = root
        .attribute("width")
        .map(length)
        .transpose()?
        .unwrap_or(view_box[2]);
    let document_height = root
        .attribute("height")
        .map(length)
        .transpose()?
        .unwrap_or(view_box[3]);
    let aspect_ratio = document_width / document_height;
    ensure!(
        aspect_ratio.is_finite() && aspect_ratio > 0.0,
        "invalid SVG aspect ratio"
    );

    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_xmltree(&document, &options).context("SVG conversion failed")?;
    // Reuse the application's capture size rather than inventing an icon limit.
    // At the current 1280 x 720 default, the longest side is 1280: a square
    // RGBA preview allocates 1280 * 1280 * 4 = 6,553,600 bytes. Source dimensions
    // never control raster allocation. Uniform scaling preserves aspect ratio.
    let capture = crate::render::browser::CaptureOptions::default();
    let preview_side = capture.width.max(capture.height);
    let (width, height) = if aspect_ratio >= 1.0 {
        (
            preview_side,
            (f64::from(preview_side) / aspect_ratio).round().max(1.0) as u32,
        )
    } else {
        (
            (f64::from(preview_side) * aspect_ratio).round().max(1.0) as u32,
            preview_side,
        )
    };
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).context("cannot allocate SVG preview")?;
    let size = tree.size();
    let scale = (width as f32 / size.width()).min(height as f32 / size.height());
    let transform = tiny_skia::Transform::from_row(
        scale,
        0.0,
        0.0,
        scale,
        (width as f32 - size.width() * scale) / 2.0,
        (height as f32 - size.height() * scale) / 2.0,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut visible = false;
    let mut touches_edge = false;
    for (index, pixel) in pixmap.pixels().iter().enumerate() {
        if pixel.alpha() == 0 {
            continue;
        }
        visible = true;
        let x = index as u32 % width;
        let y = index as u32 / width;
        touches_edge |= x == 0 || y == 0 || x == width - 1 || y == height - 1;
    }
    ensure!(visible, "SVG has no visible artwork in its viewport");
    let warnings = if touches_edge {
        vec!["Artwork reaches the preview boundary; check for possible clipping.".to_owned()]
    } else {
        Vec::new()
    };
    Ok(ValidatedSvg {
        source: source.to_owned(),
        png: pixmap
            .encode_png()
            .context("cannot encode SVG preview PNG")?,
        width,
        height,
        aspect_ratio,
        warnings,
    })
}

/// Bound nesting before either recursive parser sees the source. The root is
/// depth zero; empty elements count just like paired start/end elements.
fn preflight_xml(source: &str) -> Result<()> {
    let mut reader = Reader::from_str(source);
    let mut depth = 0usize;
    loop {
        match reader.read_event().context("invalid SVG XML")? {
            event @ (Event::Start(_) | Event::Empty(_)) => {
                ensure!(
                    depth <= MAX_SVG_DEPTH,
                    "SVG nesting budget exceeded: limit {MAX_SVG_DEPTH}, asked {depth}"
                );
                // Empty elements have no matching End event.
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::DocType(_) => anyhow::bail!("invalid SVG XML; DTDs are forbidden"),
            Event::PI(_) => anyhow::bail!("SVG processing instructions are forbidden"),
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

fn validate_element(node: roxmltree::Node<'_, '_>, root: roxmltree::Node<'_, '_>) -> Result<()> {
    let tag = node.tag_name().name();
    ensure!(
        matches!(
            tag,
            "svg" | "g" | "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon"
        ),
        "unsupported SVG element: {tag}"
    );
    ensure!(
        tag != "svg" || node == root,
        "nested SVG elements are unsupported"
    );
    if node != root {
        let parent = node.parent_element().context("missing SVG parent")?;
        ensure!(
            matches!(parent.tag_name().name(), "svg" | "g"),
            "{tag} must be inside svg or g"
        );
    }
    for attribute in node.attributes() {
        let name = attribute.name();
        let value = attribute.value().trim();
        ensure!(
            attribute.namespace().is_none(),
            "namespaced SVG attributes are forbidden: {name}"
        );
        let geometry = match tag {
            "svg" => matches!(name, "viewBox" | "width" | "height" | "preserveAspectRatio"),
            "rect" => matches!(name, "x" | "y" | "width" | "height" | "rx" | "ry"),
            "circle" => matches!(name, "cx" | "cy" | "r"),
            "ellipse" => matches!(name, "cx" | "cy" | "rx" | "ry"),
            "line" => matches!(name, "x1" | "y1" | "x2" | "y2"),
            "path" => name == "d",
            "polyline" | "polygon" => name == "points",
            "g" => false,
            _ => unreachable!("element allowlist checked above"),
        };
        let presentation = matches!(
            name,
            "id" | "fill"
                | "stroke"
                | "color"
                | "opacity"
                | "fill-opacity"
                | "stroke-opacity"
                | "stroke-width"
                | "stroke-linecap"
                | "stroke-linejoin"
                | "stroke-miterlimit"
                | "fill-rule"
                | "transform"
                | "display"
                | "visibility"
        );
        ensure!(
            geometry || presentation,
            "unsupported attribute {name} on {tag}"
        );
        match name {
            "id" => ensure!(
                !value.is_empty() && !value.chars().any(char::is_whitespace),
                "invalid SVG id"
            ),
            "viewBox" => {
                numbers(value)?;
            }
            "d" => validate_path(value)?,
            "points" => {
                let points = numbers(value)?;
                let minimum = if tag == "polygon" { 6 } else { 4 };
                ensure!(
                    points.len() >= minimum && points.len() % 2 == 0,
                    "{tag} requires complete coordinate pairs"
                );
            }
            "fill" | "stroke" => {
                ensure!(
                    value == "none"
                        || value == "currentColor"
                        || value.parse::<svgtypes::Color>().is_ok(),
                    "unsupported SVG paint: {value}"
                );
            }
            "color" => {
                value
                    .parse::<svgtypes::Color>()
                    .context("invalid SVG color")?;
            }
            "opacity" | "fill-opacity" | "stroke-opacity" => {
                let number = number(value)?;
                ensure!(
                    (0.0..=1.0).contains(&number),
                    "{name} must be between 0 and 1"
                );
                ensure!(
                    name != "opacity" || !matches!(tag, "svg" | "g") || number == 1.0,
                    "group opacity is unsupported; apply fill-opacity or stroke-opacity to shapes instead"
                );
            }
            "stroke-linecap" => ensure!(
                matches!(value, "butt" | "round" | "square"),
                "invalid stroke-linecap"
            ),
            "stroke-linejoin" => ensure!(
                matches!(value, "miter" | "round" | "bevel"),
                "invalid stroke-linejoin"
            ),
            "fill-rule" => ensure!(matches!(value, "nonzero" | "evenodd"), "invalid fill-rule"),
            "display" => ensure!(matches!(value, "inline" | "none"), "invalid display"),
            "visibility" => ensure!(
                matches!(value, "visible" | "hidden" | "collapse"),
                "invalid visibility"
            ),
            "preserveAspectRatio" => ensure!(
                matches!(value, "xMidYMid" | "xMidYMid meet"),
                "only centered, aspect-preserving viewBox fitting is supported"
            ),
            "transform" => {
                let transform = value
                    .parse::<svgtypes::Transform>()
                    .context("invalid SVG transform")?;
                ensure!(!value.is_empty(), "empty SVG transform");
                for number in [
                    transform.a,
                    transform.b,
                    transform.c,
                    transform.d,
                    transform.e,
                    transform.f,
                ] {
                    finite(number)?;
                }
            }
            "stroke-miterlimit" => ensure!(
                number(value)? >= 1.0,
                "stroke-miterlimit must be at least 1"
            ),
            _ => {
                let value = length(value)?;
                if matches!(name, "width" | "height" | "r")
                    || (tag == "ellipse" && matches!(name, "rx" | "ry"))
                {
                    ensure!(value > 0.0, "{name} must be positive");
                } else if matches!(name, "rx" | "ry" | "stroke-width") {
                    ensure!(value >= 0.0, "{name} must be nonnegative");
                }
            }
        }
    }
    let required: &[&str] = match tag {
        "svg" => &["viewBox"],
        "path" => &["d"],
        "rect" => &["width", "height"],
        "circle" => &["r"],
        "ellipse" => &["rx", "ry"],
        "polyline" | "polygon" => &["points"],
        _ => &[],
    };
    for name in required {
        ensure!(node.has_attribute(*name), "{tag} requires {name}");
    }
    Ok(())
}

fn finite(value: f64) -> Result<f64> {
    ensure!(
        value.is_finite() && (value as f32).is_finite() && (value == 0.0 || value as f32 != 0.0),
        "SVG number is outside the renderer's finite f32 range: {value}"
    );
    Ok(value)
}

fn number(value: &str) -> Result<f64> {
    let values = numbers(value)?;
    ensure!(values.len() == 1, "expected one SVG number: {value}");
    Ok(values[0])
}

fn numbers(value: &str) -> Result<Vec<f64>> {
    svgtypes::NumberListParser::from(value)
        .map(|number| finite(number.context("invalid SVG number list")?))
        .collect()
}

fn length(value: &str) -> Result<f64> {
    let length = value
        .parse::<svgtypes::Length>()
        .context("invalid SVG length")?;
    ensure!(
        matches!(
            length.unit,
            svgtypes::LengthUnit::None | svgtypes::LengthUnit::Px
        ),
        "SVG lengths must use user units or px"
    );
    finite(length.number)
}

fn validate_path(value: &str) -> Result<()> {
    use svgtypes::PathSegment;
    let mut segments = svgtypes::PathParser::from(value).peekable();
    ensure!(segments.peek().is_some(), "empty SVG path");
    let mut previous_close = false;
    for segment in segments {
        let segment = segment.context("invalid SVG path data")?;
        // svgtypes 0.16 SimplifyingPathParser skips repeated ClosePath segments
        // by recursively calling next(). Reject that redundant syntax before
        // usvg can turn a long run of Z commands into a stack overflow.
        let close = matches!(segment, PathSegment::ClosePath { .. });
        ensure!(
            !close || !previous_close,
            "consecutive close-path commands are unsupported"
        );
        previous_close = close;
        let values: &[f64] = match segment {
            PathSegment::MoveTo { x, y, .. }
            | PathSegment::LineTo { x, y, .. }
            | PathSegment::SmoothQuadratic { x, y, .. } => &[x, y],
            PathSegment::HorizontalLineTo { x, .. } => &[x],
            PathSegment::VerticalLineTo { y, .. } => &[y],
            PathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
                ..
            } => &[x1, y1, x2, y2, x, y],
            PathSegment::SmoothCurveTo { x2, y2, x, y, .. } => &[x2, y2, x, y],
            PathSegment::Quadratic { x1, y1, x, y, .. } => &[x1, y1, x, y],
            PathSegment::EllipticalArc {
                rx,
                ry,
                x_axis_rotation,
                x,
                y,
                ..
            } => {
                ensure!(rx >= 0.0 && ry >= 0.0, "arc radii must be nonnegative");
                &[rx, ry, x_axis_rotation, x, y]
            }
            PathSegment::ClosePath { .. } => &[],
        };
        for value in values {
            finite(*value)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "svg_tests.rs"]
mod tests;
