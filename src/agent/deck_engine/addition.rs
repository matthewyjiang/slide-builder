//! Bounds preconditions for semantic additions, evaluated while the mutation lock is held.
use super::{layout_contract::Rect, layout_xml, mutation, DeckMutation};
use anyhow::{anyhow, bail, Context, Result};
use pptx_handler::PptxHandler;

pub(super) fn validate(handler: &PptxHandler, operations: &[DeckMutation]) -> Result<()> {
    let size = layout_xml::slide_size(&layout_xml::presentation(handler)?)?;
    for (index, operation) in operations.iter().enumerate() {
        let DeckMutation::Add {
            element_type,
            properties,
            ..
        } = operation
        else {
            bail!("bounded additions accept only Add operations");
        };
        if element_type == "slide" {
            bail!("bounded additions require slide elements, not slides");
        }
        let distance = |key: &str| -> Result<f64> {
            let value = properties
                .get(key)
                .ok_or_else(|| anyhow!("missing addition geometry `{key}`"))?;
            Ok(mutation::value_as_emu(value)? as f64 / 914_400.0)
        };
        Rect {
            x: distance("x")?,
            y: distance("y")?,
            width: distance("width")?,
            height: distance("height")?,
        }
        .validate(size)
        .with_context(|| {
            format!(
                "edit {} has invalid geometry for {} x {} inch slide",
                index + 1,
                size.0,
                size.1
            )
        })?;
    }
    Ok(())
}
