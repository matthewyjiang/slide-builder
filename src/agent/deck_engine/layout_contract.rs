//! Persisted layout vocabulary. Distances are inches; type sizes are points.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Contract {
    pub margins: Margins,
    pub gutter: f64,
    pub regions: BTreeMap<String, Rect>,
    pub text_styles: BTreeMap<String, TextStyle>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Margins {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TextStyle {
    pub font_size: f64,
    pub font_family: String,
    pub color: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub alignment: Alignment,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Alignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}
impl TextStyle {
    pub fn properties(&self) -> std::collections::HashMap<String, String> {
        [
            ("fontSize", self.font_size.to_string()),
            ("font", self.font_family.clone()),
            ("color", self.color.clone()),
            ("bold", self.bold.to_string()),
            ("italic", self.italic.to_string()),
            (
                "alignment",
                match self.alignment {
                    Alignment::Left => "left",
                    Alignment::Center => "center",
                    Alignment::Right => "right",
                    Alignment::Justify => "justify",
                }
                .to_owned(),
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
    }
}
pub(super) fn nonnegative(value: f64, name: &str) -> Result<()> {
    if !value.is_finite() || value < 0.0 {
        bail!("{name} must be finite and nonnegative, got {value}");
    }
    Ok(())
}
impl Rect {
    pub fn validate(&self, size: (f64, f64)) -> Result<()> {
        nonnegative(self.x, "x")?;
        nonnegative(self.y, "y")?;
        if !self.width.is_finite()
            || !self.height.is_finite()
            || self.width <= 0.0
            || self.height <= 0.0
        {
            bail!("width and height must be finite and positive");
        }
        // Each attribute is rounded independently when written to the package.
        let width = emu(self.width)?;
        let height = emu(self.height)?;
        if width <= 0 || height <= 0 {
            bail!("width and height must occupy at least one OOXML EMU");
        }
        if i128::from(emu(self.x)?) + i128::from(width) > i128::from(emu(size.0)?)
            || i128::from(emu(self.y)?) + i128::from(height) > i128::from(emu(size.1)?)
        {
            bail!(
                "geometry ({}, {}, {}, {}) exceeds slide bounds {} x {} inches",
                self.x,
                self.y,
                self.width,
                self.height,
                size.0,
                size.1
            );
        }
        Ok(())
    }
}
pub(super) fn emu(value: f64) -> Result<i64> {
    let scaled = (value * 914_400.0).round();
    if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled >= i64::MAX as f64 {
        bail!("distance {value} inches exceeds OOXML signed 64-bit EMU representation");
    }
    Ok(scaled as i64)
}
impl Contract {
    pub fn normalize(&mut self, size: (f64, f64)) -> Result<()> {
        for (name, value) in [
            ("left", self.margins.left),
            ("right", self.margins.right),
            ("top", self.margins.top),
            ("bottom", self.margins.bottom),
            ("gutter", self.gutter),
        ] {
            nonnegative(value, name)?;
        }
        if self.margins.left + self.margins.right >= size.0
            || self.margins.top + self.margins.bottom >= size.1
        {
            bail!(
                "margins leave no positive content area on {} x {} inch slide",
                size.0,
                size.1
            );
        }
        for (name, region) in &self.regions {
            if name.trim().is_empty() {
                bail!("region names must not be empty");
            }
            region.validate(size)?;
        }
        for (name, style) in &mut self.text_styles {
            if name.trim().is_empty() || style.font_family.trim().is_empty() {
                bail!("style names and font families must not be empty");
            }
            // DrawingML ST_TextFontSize range, in hundredths of a point.
            if !style.font_size.is_finite() || !(1.0..=4000.0).contains(&style.font_size) {
                bail!("style {name}: font_size must be within OOXML range 1..=4000 points");
            }
            style.font_size = (style.font_size * 100.0).round() / 100.0;
            style.color = style
                .color
                .strip_prefix('#')
                .unwrap_or(&style.color)
                .to_ascii_uppercase();
            if style.color.len() != 6 || !style.color.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("style {name}: color must be six hexadecimal RGB digits");
            }
        }
        Ok(())
    }
}
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Metadata {
    pub contract: Option<Contract>,
    #[serde(default)]
    pub assignments: BTreeMap<String, String>,
    #[serde(default)]
    pub geometry_rules: Vec<super::layout_plan::Operation>,
}
