---
name: slide-builder-pptx
description: Build and edit PowerPoint decks safely with slide-builder's native semantic tools, validation, rendering, and visual QA workflow.
---
# Native PPTX authoring

Use the semantic native deck tools first. They operate on the active deck, enforce slide bounds and payload limits, preserve stable element IDs, validate mutations, and publish changes transactionally.

## Workflow

1. Inspect the deck and identify its slide size, theme, layouts, and existing element IDs.
2. Plan a coherent narrative and reusable layout system before adding content.
3. Use `slide_create`, `slide_duplicate`, `slide_delete`, and `slide_reorder` for structure.
4. Use `text_add`, `image_add`, and `shape_add` for content. Keep returned stable IDs.
5. Use `element_update` with a stable ID instead of positional or ambiguous selectors.
6. Run `deck_validate` after meaningful edits.
7. Run `render_deck`, inspect every slide, and use `set_active_slide` to synchronize the UI.
8. Ask the user to attach the active slide with Ctrl+V when you need visual feedback. Fix clipping, overlap, contrast, alignment, and hierarchy, then render again.

## Coordinates and layout

Use the coordinate units reported by `deck_inspect`. Never guess the slide dimensions. Keep every element inside slide bounds and reserve consistent margins. Prefer alignment, grids, whitespace, and a small type scale over dense decoration. Use theme colors and fonts where possible. Crop images intentionally and preserve aspect ratio unless distortion is explicitly desired.

## Transactions and validation

Mutations are committed only after validation and package reopen checks. A failed operation leaves the original deck intact. Treat returned IDs and post-state as authoritative. Do not retry a failed mutation blindly: inspect the error and current deck state first.

## Advanced operations

Use advanced path-based add, set, remove, move, copy, swap, query, view, or extract operations only when the semantic surface cannot express the required change. Use `raw` or `raw_set` only as a last resort for a precisely understood OOXML operation. Inspect the target part and relationships, make the smallest change, validate, reopen, and render. Never use raw XML merely to avoid learning a semantic tool.

## Optional OfficeCLI compatibility

If and only if a native operation explicitly reports that it is unsupported, you may explain an equivalent external `officecli` command. Never invoke it silently. It is optional, may not be installed, and runs as an approval-gated process. Native deck tools remain the default and require no process approval.
