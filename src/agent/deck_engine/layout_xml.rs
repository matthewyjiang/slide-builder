//! XML element tree preserving child events and unknown properties during layout edits.
use super::{
    layout_contract::{emu, Metadata, Rect},
    DeckMutation,
};
use anyhow::{anyhow, bail, Result};
use handler_common::{output_format::RawOptions, DocumentHandler};
use pptx_handler::PptxHandler;
use quick_xml::{
    events::{BytesStart, BytesText, Event},
    Reader, Writer,
};

pub(super) const NAMESPACE: &str = "urn:slide-builder:layout:v1";
pub(super) const PRESENTATION: &str = "ppt/presentation.xml";
#[derive(Clone)]
pub(super) enum Node {
    Element(Element),
    Other(Event<'static>),
}
#[derive(Clone)]
pub(super) struct Element {
    pub start: BytesStart<'static>,
    pub children: Vec<Node>,
}
impl Element {
    pub fn new(name: &str) -> Self {
        Self {
            start: BytesStart::new(name.to_owned()),
            children: vec![],
        }
    }
    pub fn named(&self, name: &str) -> bool {
        self.start.local_name().as_ref() == name.as_bytes()
    }
    pub fn attr(&self, name: &str) -> Option<String> {
        self.start
            .attributes()
            .filter_map(Result::ok)
            .find(|a| a.key.as_ref() == name.as_bytes())
            .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
    }
    pub fn set_attr(&mut self, name: &str, value: &str) -> Result<()> {
        let attrs = self
            .start
            .attributes()
            .map(|a| {
                let a = a?;
                Ok((
                    String::from_utf8(a.key.as_ref().to_vec())?,
                    a.unescape_value()?.into_owned(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        self.start.clear_attributes();
        for (k, v) in &attrs {
            if k != name {
                self.start.push_attribute((k.as_str(), v.as_str()));
            }
        }
        self.start.push_attribute((name, value));
        Ok(())
    }
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Other(_) => None,
        })
    }
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.named(name))
    }
    pub fn child_mut(&mut self, name: &str) -> Option<&mut Element> {
        self.children.iter_mut().find_map(|n| match n {
            Node::Element(e) if e.named(name) => Some(e),
            _ => None,
        })
    }
    pub fn descendant(&self, name: &str) -> Option<&Element> {
        if self.named(name) {
            return Some(self);
        }
        self.elements().find_map(|e| e.descendant(name))
    }
    pub fn xml(&self) -> Result<String> {
        fn emit(e: &Element, w: &mut Writer<Vec<u8>>) -> Result<()> {
            if e.children.is_empty() {
                w.write_event(Event::Empty(e.start.borrow()))?;
            } else {
                w.write_event(Event::Start(e.start.borrow()))?;
                for n in &e.children {
                    match n {
                        Node::Element(e) => emit(e, w)?,
                        Node::Other(v) => w.write_event(v.borrow())?,
                    }
                }
                w.write_event(Event::End(e.start.to_end()))?;
            }
            Ok(())
        }
        let mut w = Writer::new(vec![]);
        emit(self, &mut w)?;
        Ok(String::from_utf8(w.into_inner())?)
    }
}
pub(super) fn parse(xml: &str) -> Result<Element> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Element> = vec![];
    let mut root = None;
    loop {
        let event = reader.read_event()?;
        match &event {
            Event::Start(start) | Event::Empty(start) => {
                for attr in start.attributes() {
                    attr?.unescape_value()?;
                }
            }
            Event::Text(text) => {
                let value = text.unescape()?;
                if stack.is_empty() && !value.trim().is_empty() {
                    bail!("text outside XML root");
                }
            }
            _ => {}
        }
        match event {
            Event::Start(start) => stack.push(Element {
                start: start.into_owned(),
                children: vec![],
            }),
            Event::Empty(start) => {
                let e = Element {
                    start: start.into_owned(),
                    children: vec![],
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(Node::Element(e));
                } else if root.replace(e).is_some() {
                    bail!("multiple XML roots");
                }
            }
            Event::End(_) => {
                let e = stack.pop().ok_or_else(|| anyhow!("unbalanced XML"))?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(Node::Element(e));
                } else if root.replace(e).is_some() {
                    bail!("multiple XML roots");
                }
            }
            Event::Eof => break,
            event => {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(Node::Other(event.into_owned()));
                }
            }
        }
    }
    if !stack.is_empty() {
        bail!("unclosed XML element");
    }
    root.ok_or_else(|| anyhow!("missing XML root"))
}
pub(super) fn presentation(handler: &PptxHandler) -> Result<Element> {
    parse(&handler.raw(PRESENTATION, RawOptions::default())?)
}
pub(super) fn slide_size(root: &Element) -> Result<(f64, f64)> {
    let size = root
        .child("sldSz")
        .ok_or_else(|| anyhow!("presentation has no slide size"))?;
    let w = number(size, "cx")? / 914_400.0;
    let h = number(size, "cy")? / 914_400.0;
    if w <= 0.0 || h <= 0.0 {
        bail!("slide dimensions must be positive");
    }
    Ok((w, h))
}
pub(super) fn number(e: &Element, key: &str) -> Result<f64> {
    let v = e
        .attr(key)
        .ok_or_else(|| anyhow!("missing XML attribute {key}"))?
        .parse::<i64>()?;
    Ok(v as f64)
}
pub(super) fn metadata(root: &Element) -> Result<Metadata> {
    let Some(ext) = root.child("extLst").and_then(|list| {
        list.elements()
            .find(|e| e.named("ext") && e.attr("uri").as_deref() == Some(NAMESPACE))
    }) else {
        return Ok(Metadata::default());
    };
    let data = ext
        .child("layout")
        .ok_or_else(|| anyhow!("layout metadata is missing its payload"))?;
    let mut text = String::new();
    for node in &data.children {
        match node {
            Node::Other(Event::Text(t)) => text.push_str(&t.unescape()?),
            _ => bail!("invalid layout metadata content"),
        }
    }
    Ok(serde_json::from_str(&text)?)
}
pub(super) fn metadata_mutation(mut root: Element, metadata: &Metadata) -> Result<DeckMutation> {
    const PRESENTATION_NAMESPACE: &str =
        "http://schemas.openxmlformats.org/presentationml/2006/main";
    match root.attr("xmlns:p") {
        Some(namespace) if namespace != PRESENTATION_NAMESPACE => {
            bail!("presentation binds p: to an unexpected namespace")
        }
        Some(_) => {}
        None => root.set_attr("xmlns:p", PRESENTATION_NAMESPACE)?,
    }
    let mut ext = Element::new("p:ext");
    ext.set_attr("uri", NAMESPACE)?;
    let mut data = Element::new("sb:layout");
    data.set_attr("xmlns:sb", NAMESPACE)?;
    data.children.push(Node::Other(Event::Text(
        BytesText::new(&serde_json::to_string(metadata)?).into_owned(),
    )));
    ext.children.push(Node::Element(data));
    if root.child("extLst").is_none() {
        root.children.push(Node::Element(Element::new("p:extLst")));
    }
    let list = root.child_mut("extLst").unwrap();
    list.children.retain(|n|!matches!(n,Node::Element(e) if e.named("ext") && e.attr("uri").as_deref()==Some(NAMESPACE)));
    list.children.push(Node::Element(ext));
    Ok(DeckMutation::RawSet {
        part: PRESENTATION.to_owned(),
        xpath: "/presentation".into(),
        action: "replace".into(),
        xml: Some(root.xml()?),
    })
}
pub(super) fn rect(shape: &Element) -> Result<Rect> {
    let transform = shape
        .child("spPr")
        .and_then(|p| p.child("xfrm"))
        .ok_or_else(|| anyhow!("element has no explicit transform"))?;
    let off = transform
        .child("off")
        .ok_or_else(|| anyhow!("transform has no offset"))?;
    let ext = transform
        .child("ext")
        .ok_or_else(|| anyhow!("transform has no extent"))?;
    Ok(Rect {
        x: number(off, "x")? / 914_400.0,
        y: number(off, "y")? / 914_400.0,
        width: number(ext, "cx")? / 914_400.0,
        height: number(ext, "cy")? / 914_400.0,
    })
}
pub(super) fn set_rect(shape: &mut Element, r: Rect) -> Result<()> {
    let transform = shape
        .child_mut("spPr")
        .and_then(|p| p.child_mut("xfrm"))
        .ok_or_else(|| anyhow!("element has no explicit transform"))?;
    let off = transform
        .child_mut("off")
        .ok_or_else(|| anyhow!("missing offset"))?;
    off.set_attr("x", &emu(r.x)?.to_string())?;
    off.set_attr("y", &emu(r.y)?.to_string())?;
    let ext = transform
        .child_mut("ext")
        .ok_or_else(|| anyhow!("missing extent"))?;
    ext.set_attr("cx", &emu(r.width)?.to_string())?;
    ext.set_attr("cy", &emu(r.height)?.to_string())?;
    Ok(())
}
