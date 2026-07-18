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
7. Run `render_deck` and wait for its completed image-path result, then use `set_active_slide` to synchronize the UI while reviewing slides. Rendered images appear in the TUI but are not attached to the model, so do not claim visual inspection unless the user attaches a slide with Ctrl+V.
8. Ask the user to attach the active slide with Ctrl+V when you need visual feedback. Fix clipping, overlap, contrast, alignment, and hierarchy, then render again.

## Coordinates and layout

Use the coordinate units reported by `deck_inspect`. Never guess the slide dimensions. Keep every element inside slide bounds and reserve consistent margins. Prefer alignment, grids, whitespace, and a small type scale over dense decoration. Use theme colors and fonts where possible. Crop images intentionally and preserve aspect ratio unless distortion is explicitly desired.

## Transactions and validation

Mutations are committed only after validation and package reopen checks. A failed operation leaves the original deck intact. Treat returned IDs and post-state as authoritative. Do not retry a failed mutation blindly: inspect the error and current deck state first.

## Tool payloads

Use these exact argument shapes. Slide indexes are one-based. Geometry values are inches.

- `deck_inspect`: `{}` for the whole deck, or `{"path":"/slide[2]"}` for a specific handler path. Results include slide size and shape geometry in inches.
- `slide_create`: `{}`.
- `slide_duplicate` and `slide_delete`: `{"index":2}`.
- `slide_reorder`: `{"from":4,"to":2}`.
- `text_add`: `{"slide":2,"text":"Label","x":1.0,"y":1.0,"width":3.0,"height":0.6,"font_size":24}`.
- `image_add`: `{"slide":2,"path":"/absolute/image.png","x":1.0,"y":1.0,"width":4.0,"height":3.0}`.
- `shape_add`: `{"slide":2,"kind":"rectangle","x":1.0,"y":1.0,"width":4.0,"height":1.0,"fill":"#336699"}`.
- `element_update`: `{"id":"<stable ID>","properties":{"text":"Replacement text","font_size":"24"}}`. Put every changed value inside `properties`. Do not send `path` to this tool.
- `deck_validate`: `{}`.

Prefer `element_update` for an existing element and the dedicated add tools for new content. Do not use `deck_advanced` when one of those tools can perform the edit.

## Advanced operations

`deck_advanced` accepts exactly one `mutation` object. Fields such as `path` and `properties` belong inside `mutation`, not at the top level.

```json
{"mutation":{"operation":"set","path":"/slide[2]/shape[4]","properties":{"text":"Replacement text"}}}
```

The supported advanced operations are:

- `add`: requires `parent`, `element_type`, and optional string `properties`.
- `set`: requires `path` and string `properties`.
- `remove`: requires `path`.
- `move`: requires `source`, with optional `target_parent` and `index`.
- `copy`: requires `source` and `target_parent`, with optional `index`.
- `swap`: requires `left` and `right`.
- `raw_set`: requires `part`, `xpath`, and `action`, with optional `xml`.

Use `raw_set` only as a last resort for a precisely understood OOXML operation. Inspect the target part and relationships, make the smallest change, validate, reopen, and render. Never use raw XML merely to avoid a semantic tool.

## Optional OfficeCLI compatibility

If and only if a native operation explicitly reports that it is unsupported, you may explain an equivalent external `officecli` command. Never invoke it silently. It is optional, may not be installed, and runs as an approval-gated process. Native deck tools remain the default and require no process approval.
