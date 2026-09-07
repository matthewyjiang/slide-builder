---
name: deck-assets
description: Generate clear charts, diagrams, and repository-derived visual assets at presentation resolution and embed them safely in decks.
---
# Deck assets

Inspect the host repository for source material before inventing an asset. Reads are allowed, but writes and commands outside the deck area may require approval. Keep generated assets deterministic and record their source data or generation logic when practical.

## Charts

Use a chart only when the data supports a comparison or trend. Prefer direct labels, honest axes, accessible colors, and low visual noise. Generate raster charts with matplotlib at the final slide aspect and sufficient pixel density for the configured render size. Use PNG with transparency when appropriate. Avoid screenshots of charts and avoid tiny legends.

## Diagrams

For architecture, flow, and relationships, choose between:

- native shapes through deck tools when editability, theme integration, and simple geometry matter;
- Mermaid rendered to PNG when the diagram is complex and a renderer is available;
- a programmatically generated PNG when exact custom visualization is required.

Keep labels short, make reading order obvious, and avoid crossing connectors. Render and inspect the result at slide size.

## Embedding

Store assets in an approved location, use stable descriptive names, preserve aspect ratios, and embed rather than link remote resources. Match the active design package's colors and typography without sacrificing contrast. Do not fetch untrusted remote assets or depend on a CDN. After insertion, validate the deck and use the render-to-attach-to-fix loop to catch scaling, clipping, font, and transparency problems.

## Managed SVG illustrations

Use `asset_list` before creating custom icons or conceptual illustrations. Author
SVG with `asset_create_svg`, supplying a name and a brief with `purpose`, `style`,
and `alt_text`. Use simple shapes, groups, paths, solid fills/strokes, and an
explicit positive `viewBox`. Text, CSS, gradients, images, references, filters,
dashed strokes, and translucent groups are unsupported. Keep labels native.

The tool validates the source and attaches a PNG preview. Inspect it for visible
defects and compliance with the brief; a passing gate does not prove quality.
`asset_inspect` takes an `id` and returns source plus a fresh preview. To revise,
call `asset_create_svg` with `revises` set to the old ID. Do not overwrite stored
records. `asset_place` takes `id`, `slide`, `x`, `y`, `width`, and `height`; it
centers the image in the box without distortion and embeds SVG with PNG fallback.

Every placement requires `render_deck` and an in-slide check for contrast,
legibility, crop, hierarchy, style consistency, and meaning. Give concrete defect
feedback instead of a beauty score. Do not claim approval for unseen images.
These assets are picture objects, not editable PowerPoint shapes. Never use a
generated illustration as experimental evidence or a photo of actual equipment.
Raster AI image generation is not available.
