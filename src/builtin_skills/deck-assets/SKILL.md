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
