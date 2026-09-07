//! Deck-local, immutable SVG candidates. Validation is not visual approval.
mod store;

pub(super) use crate::render::svg::{validate_and_render, ValidatedSvg};
pub(super) use store::{AssetRecord, AssetStore, CreateAsset};
