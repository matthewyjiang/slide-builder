//! Compatibility layer for text formatting omitted or misplaced by the pinned handler.
//! Keep XML events intact except for the requested attributes and property children.
use anyhow::{anyhow, bail, Context, Result};
use handler_common::{output_format::RawOptions, DocumentHandler};
use pptx_handler::PptxHandler;
use quick_xml::{
    events::{BytesStart, Event},
    Reader, Writer,
};
use std::collections::HashMap;

pub(super) fn take(properties: &mut HashMap<String, String>) -> HashMap<String, String> {
    [
        "bold",
        "italic",
        "font",
        "fontName",
        "fontSize",
        "color",
        "fontColor",
        "alignment",
    ]
    .into_iter()
    .filter_map(|key| properties.remove(key).map(|value| (key.to_owned(), value)))
    .collect()
}

pub(super) fn set(
    handler: &PptxHandler,
    path: &str,
    properties: &HashMap<String, String>,
) -> Result<()> {
    let part = super::slide_part(handler, super::slide_index(path)?)?;
    let shape = path
        .split("/shape[")
        .nth(1)
        .and_then(|value| value.strip_suffix(']'))
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|index| *index > 0)
        .ok_or_else(|| anyhow!("text formatting requires /slide[N]/shape[M]"))?;
    let xml = handler.raw(&part, RawOptions::default())?;
    let xml = format_shape(&xml, shape, properties)?;
    super::replace_slide_document(handler, &part, &xml)
}

// A small event tree preserves comments, whitespace, prefixes, and unknown XML.
// Unlike a generic DOM serializer it does not normalize unrelated attributes.
enum Node {
    Element(Element),
    Other(Event<'static>),
}

struct Element {
    start: BytesStart<'static>,
    children: Vec<Node>,
    empty: bool,
}

impl Element {
    fn new(name: &str) -> Self {
        Self {
            start: BytesStart::new(name.to_owned()),
            children: Vec::new(),
            empty: true,
        }
    }

    fn named(&self, name: &str) -> bool {
        self.start.name().as_ref() == name.as_bytes()
    }

    fn attr(&mut self, name: &str, value: &str) -> Result<()> {
        let attributes = self
            .start
            .attributes()
            .filter_map(|attribute| match attribute {
                Ok(attribute) if attribute.key.as_ref() == name.as_bytes() => None,
                other => Some(other.map(|attribute| {
                    (
                        attribute.key.as_ref().to_vec(),
                        attribute.value.into_owned(),
                    )
                })),
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        self.start.clear_attributes();
        for (key, value) in &attributes {
            self.start
                .push_attribute((key.as_slice(), value.as_slice()));
        }
        self.start.push_attribute((name, value));
        Ok(())
    }

    fn child(&mut self, name: &str) -> &mut Element {
        let index = self
            .children
            .iter()
            .position(|node| matches!(node, Node::Element(element) if element.named(name)))
            .unwrap_or_else(|| {
                self.children.insert(0, Node::Element(Self::new(name)));
                0
            });
        match &mut self.children[index] {
            Node::Element(element) => element,
            Node::Other(_) => unreachable!(),
        }
    }

    fn write(&self, writer: &mut Writer<Vec<u8>>) -> Result<()> {
        if self.empty && self.children.is_empty() {
            writer.write_event(Event::Empty(self.start.borrow()))?;
        } else {
            writer.write_event(Event::Start(self.start.borrow()))?;
            write_nodes(&self.children, writer)?;
            writer.write_event(Event::End(self.start.to_end()))?;
        }
        Ok(())
    }
}

fn parse(reader: &mut Reader<&[u8]>) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(start) => nodes.push(Node::Element(Element {
                start: start.into_owned(),
                children: parse(reader)?,
                empty: false,
            })),
            Event::Empty(start) => nodes.push(Node::Element(Element {
                start: start.into_owned(),
                children: Vec::new(),
                empty: true,
            })),
            Event::End(_) | Event::Eof => return Ok(nodes),
            event => nodes.push(Node::Other(event.into_owned())),
        }
    }
}

fn write_nodes(nodes: &[Node], writer: &mut Writer<Vec<u8>>) -> Result<()> {
    for node in nodes {
        match node {
            Node::Element(element) => element.write(writer)?,
            Node::Other(event) => writer.write_event(event.borrow())?,
        }
    }
    Ok(())
}

fn format_shape(xml: &str, index: usize, properties: &HashMap<String, String>) -> Result<String> {
    let style = Style::parse(properties)?;
    let mut reader = Reader::from_str(xml);
    let mut nodes = parse(&mut reader)?;
    fn check_prefixes(nodes: &[Node]) -> Result<()> {
        for node in nodes {
            if let Node::Element(element) = node {
                for attribute in element.start.attributes() {
                    let attribute = attribute?;
                    let expected = match attribute.value.as_ref() {
                        b"http://schemas.openxmlformats.org/drawingml/2006/main" => {
                            Some(b"xmlns:a".as_slice())
                        }
                        b"http://schemas.openxmlformats.org/presentationml/2006/main" => {
                            Some(b"xmlns:p".as_slice())
                        }
                        _ => None,
                    };
                    if let Some(expected) = expected {
                        if attribute.key.as_ref() != expected {
                            bail!("text formatting currently requires canonical p: and a: XML namespace prefixes");
                        }
                    }
                }
                check_prefixes(&element.children)?;
            }
        }
        Ok(())
    }
    check_prefixes(&nodes)?;
    fn visit(nodes: &mut [Node], remaining: &mut usize, style: &Style) -> Result<bool> {
        for node in nodes {
            if let Node::Element(element) = node {
                if element.named("p:spTree") {
                    // Handler selectors omit grouped shapes and count only direct shapes.
                    for child in &mut element.children {
                        if let Node::Element(shape) = child {
                            if shape.named("p:sp") {
                                *remaining -= 1;
                                if *remaining == 0 {
                                    format_text(shape, style)?;
                                    return Ok(true);
                                }
                            }
                        }
                    }
                    return Ok(false);
                }
                if visit(&mut element.children, remaining, style)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    let mut remaining = index;
    if !visit(&mut nodes, &mut remaining, &style)? {
        bail!("shape {index} was not found for text formatting");
    }
    let mut writer = Writer::new(Vec::new());
    write_nodes(&nodes, &mut writer)?;
    Ok(String::from_utf8(writer.into_inner())?)
}

struct Style<'a> {
    attributes: Vec<(&'static str, String)>,
    color: Option<&'a str>,
    font: Option<&'a str>,
    alignment: Option<&'static str>,
}

impl<'a> Style<'a> {
    fn parse(properties: &'a HashMap<String, String>) -> Result<Self> {
        let mut attributes = Vec::new();
        for (key, xml_key) in [("bold", "b"), ("italic", "i")] {
            if let Some(value) = properties.get(key) {
                let value = match value.to_ascii_lowercase().as_str() {
                    "true" | "1" => "1",
                    "false" | "0" => "0",
                    _ => bail!("invalid {key} value `{value}`: expected true or false"),
                };
                attributes.push((xml_key, value.to_owned()));
            }
        }
        if let Some(value) = properties.get("fontSize") {
            let size = value
                .trim()
                .trim_end_matches("pt")
                .parse::<f64>()
                .with_context(|| format!("invalid fontSize `{value}`"))?;
            // DrawingML ST_TextFontSize is in hundredths of a point, 1..4000 pt.
            if !size.is_finite() || !(1.0..=4000.0).contains(&size) {
                bail!("fontSize must be within 1..=4000 pt, got `{value}`");
            }
            attributes.push(("sz", format!("{:.0}", size * 100.0)));
        }
        let color = properties
            .get("color")
            .or_else(|| properties.get("fontColor"))
            .map(|value| value.trim_start_matches('#'));
        if let Some(color) = color {
            if color.len() != 6 || !color.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                bail!("text color must be six hexadecimal RGB digits, got `{color}`");
            }
        }
        let alignment = properties
            .get("alignment")
            .map(|value| match value.as_str() {
                "left" | "l" => Ok("l"),
                "center" | "ctr" => Ok("ctr"),
                "right" | "r" => Ok("r"),
                "justify" | "just" => Ok("just"),
                _ => bail!("invalid text alignment `{value}`"),
            })
            .transpose()?;
        Ok(Self {
            attributes,
            color,
            font: properties
                .get("font")
                .or_else(|| properties.get("fontName"))
                .map(String::as_str),
            alignment,
        })
    }

    fn apply_run(&self, properties: &mut Element) -> Result<()> {
        for (name, value) in &self.attributes {
            properties.attr(name, value)?;
        }
        if let Some(color) = self.color {
            properties.children.retain(|node| !matches!(node, Node::Element(element)
                if ["a:noFill", "a:solidFill", "a:gradFill", "a:blipFill", "a:pattFill", "a:grpFill"].iter().any(|name| element.named(name))));
            let mut fill = Element::new("a:solidFill");
            fill.child("a:srgbClr").attr("val", color)?;
            // Fill follows the optional outline and precedes effects and typefaces.
            let index = properties
                .children
                .iter()
                .position(|node| matches!(node, Node::Element(element) if !element.named("a:ln")))
                .unwrap_or(properties.children.len());
            properties.children.insert(index, Node::Element(fill));
        }
        if let Some(font) = self.font {
            // Latin, East Asian and complex-script runs should use the requested family.
            for name in ["a:latin", "a:ea", "a:cs"] {
                if let Some(Node::Element(element)) = properties
                    .children
                    .iter_mut()
                    .find(|node| matches!(node, Node::Element(element) if element.named(name)))
                {
                    element.attr("typeface", font)?;
                } else {
                    let mut element = Element::new(name);
                    element.attr("typeface", font)?;
                    let order = |element: &Element| match element.start.name().as_ref() {
                        b"a:latin" => 1,
                        b"a:ea" => 2,
                        b"a:cs" => 3,
                        b"a:sym" | b"a:hlinkClick" | b"a:hlinkMouseOver" | b"a:rtl"
                        | b"a:extLst" => 4,
                        _ => 0,
                    };
                    let index = properties.children.iter().position(|node| matches!(node, Node::Element(child) if order(child) > order(&element)))
                        .unwrap_or(properties.children.len());
                    properties.children.insert(index, Node::Element(element));
                }
            }
        }
        Ok(())
    }
}

fn format_text(element: &mut Element, style: &Style) -> Result<()> {
    if element.named("a:p") {
        if let Some(alignment) = style.alignment {
            element.child("a:pPr").attr("algn", alignment)?;
        }
    }
    let has_run_style =
        !style.attributes.is_empty() || style.color.is_some() || style.font.is_some();
    if has_run_style && (element.named("a:r") || element.named("a:fld") || element.named("a:br")) {
        if style.color.is_some() {
            // Repair fills previously written beside rPr by the pinned handler.
            element
                .children
                .retain(|node| !matches!(node, Node::Element(child) if child.named("a:solidFill")));
        }
        style.apply_run(element.child("a:rPr"))?;
    } else if element.named("a:defRPr") || element.named("a:endParaRPr") {
        style.apply_run(element)?;
    }
    for node in &mut element.children {
        if let Node::Element(child) = node {
            format_text(child, style)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "text_format_tests.rs"]
mod tests;
