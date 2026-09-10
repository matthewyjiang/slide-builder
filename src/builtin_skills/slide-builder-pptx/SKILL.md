---
name: slide-builder-pptx
description: Build and edit PowerPoint decks safely with slide-builder's native semantic tools, validation, rendering, and visual QA workflow.
---
# Native PPTX authoring

Use the semantic native deck tools first. They operate on the active deck, enforce payload limits, preserve stable element IDs, validate mutations, and publish changes transactionally. Additions and layout operations check slide bounds; run the layout audit after freeform element updates and advanced edits as well.

## Workflow

1. Inspect the deck and identify its slide size, theme, layouts, and existing element IDs.
2. Load `slide-design` when composing or visually refining slides. Identify each slide's takeaway, primary visual, and supporting evidence. Inspect `deck_layout_inspect` and reuse its saved settings. If no contract exists, resolve margins, gutters, named regions, and text styles from the active design package or existing deck, then save them with `deck_layout_set`. For a new blank deck without a package, establish a restrained system and check its sizes in a rendered representative slide before extending the deck.
3. Use `slide_create`, `slide_duplicate`, `slide_delete`, and `slide_reorder` for structure.
4. Use `text_add`, `image_add`, and `shape_add` for content. Keep returned stable IDs.
5. Use `elements_layout` to establish shared edges, equal gaps, matching dimensions, region placement, and named text styles. Use `element_update` for individual content edits with a stable ID instead of positional or ambiguous selectors.
6. Run `deck_validate` after meaningful edits and `deck_layout_audit` before visual review. Inspect all returned issues, warnings, and coverage limitations, not merely tool success or the `valid` flag. These checks do not establish visual quality or prove that all text fits.
7. Run `render_deck` and wait for completion. The rendered slide images are attached automatically before your next response, in the order listed in the tool result. Inspect those images, not just the returned paths. Use `set_active_slide` to synchronize the UI while discussing a slide.
8. Before reporting completion, fix text overflow, unintended overlaps, uneven peer gaps, contrast, alignment, and hierarchy without waiting for user feedback. Repeat the audit after repairs, then render again and inspect the changed output. Text or font edits can break fit even if the box has not moved. Preserve intentional backgrounds and layering. If rendering or image delivery fails, or defects remain, report what is incomplete; never claim to have inspected an image you did not receive. The user can still attach the active slide with Ctrl+V for targeted feedback.

For posters and other single-slide documents, follow the poster workflow in `slide-design`. Keep the requested canvas and slide count, establish columns and shared gutters before adding content, and inspect dense sections at readable detail. Do not move required content to notes or extra slides to make the layout fit.

## Coordinates and layout

Use the coordinate units reported by `deck_inspect`. Never guess the slide dimensions. Keep every element inside slide bounds and reserve consistent margins. Prefer alignment, grids, whitespace, and a small type scale over dense decoration. Use theme colors and fonts where possible. Crop images intentionally and preserve aspect ratio unless distortion is explicitly desired.

## Shared layout tools

`deck_layout_inspect` takes `{}` and returns the saved `contract`, text-style `assignments`, `geometry_rules`, and exact `slide_size`. A missing contract is `null`, not a license to guess the slide dimensions. The contract travels inside the PowerPoint file and survives reopening; it is separate from the terminal application's `DESIGN.md`.

`deck_layout_set` takes a complete contract. This example illustrates the shape of a contract for a slide with room for these regions; it is not a universal design or a minimum font-size rule:

```json
{
  "margins": {"left":0.75,"right":0.75,"top":0.5,"bottom":0.5},
  "gutter": 0.5,
  "regions": {
    "headline": {"x":0.75,"y":0.5,"width":11,"height":0.8},
    "evidence": {"x":0.75,"y":2,"width":11,"height":4.5}
  },
  "text_styles": {
    "headline": {"font_size":40,"font_family":"Arial","color":"#142837","bold":true},
    "body": {"font_size":24,"font_family":"Arial","color":"#243746"}
  }
}
```

Choose the actual values from the package, slide dimensions, content, and rendered evidence. Style names and region names are yours to define. Each text style requires `font_size`, `font_family`, and RGB `color`; omitted `bold` and `italic` become false and omitted `alignment` becomes left. Setting the contract replaces settings but does not restyle or reposition existing content. Apply the relevant styles or placements afterward.

`elements_layout` takes one operation with stable IDs. The complete operation is atomic; it is not wrapped in `edits`:

- Align shared edges: `{"operation":"align","ids":["<a>","<b>"],"edge":"left"}`. Edges are `left`, `right`, `top`, `bottom`, `center_x`, or `center_y`. An optional `reference` ID anchors the alignment; otherwise the selection's bounds determine the target.
- Equalize gaps: `{"operation":"distribute","ids":["<a>","<b>","<c>"],"axis":"horizontal"}`. Spatial order is preserved. Without `gap`, the outer endpoints stay fixed; with an explicit gap in inches, the first element anchors the sequence.
- Match dimensions: `{"operation":"match_size","ids":["<b>","<c>"],"reference":"<a>","dimension":"width"}`. Dimension can also be `height` or `both`.
- Fill a region: `{"operation":"place","ids":["<a>","<b>"],"region":"evidence","axis":"horizontal"}`. Elements fill equal slots in supplied ID order, using the saved gutter unless `gap` is explicit. This operation resizes elements; use it for text and shape groups, and do not stretch an image unless intended. A single ID fills the region.
- Apply hierarchy: `{"operation":"text_style","ids":["<title>"],"style":"headline"}`. Applies the complete style to every text run in each selected box; do not use it when existing mixed emphasis must be preserved.
- Release a relationship: `{"operation":"release","ids":["<a>"]}`. Removes saved geometry rules that select or reference the IDs, plus their text-style assignments, without moving or restyling content. Deleted IDs are accepted so stale checks can be removed.

Geometry selections and references must belong to one slide. Text styles can span slides. Use `vertical` for vertically distributed or placed items. Missing IDs, duplicate IDs, impossible spacing, unknown names, and out-of-slide results fail without changing the deck. For a grid, define row regions and place each row, rather than estimating cell coordinates independently.

Geometry operations save their declared relationships for audit. A new operation supersedes earlier rules that control the same coordinates or dimensions on overlapping selections. Superseding removes the whole previous relationship, including its other members; it does not create partial subgroups. Unrelated rules remain, so matching widths after distribution may require distributing again. Moving only a reference element leaves its relationships intact and lets the audit report the drift. To intentionally detach elements from a relationship, use `release` rather than repeatedly ignoring its warnings.

`deck_layout_audit` takes `{}`. It checks slide bounds, margins, assigned text styles, and retained geometry rules against the saved file. A changed region or manually moved element can produce a rule departure; inspect the reported rule, then reapply it or release it if the relationship no longer belongs. `valid` reflects only these mechanical `issues`; also read `warnings` and `limitations`.

`possible_overlap` warnings identify intersecting unrotated element rectangles on the same slide, including contained text-on-text. Background containment and touching edges do not trigger warnings. `possible_text_overflow` compares an estimate from declared font sizes and explicit line breaks, including literal newlines, with the box's inner height. It does not measure actual line height, automatic wrapping, glyph widths, or font substitution. Rotated shapes and unsupported text layouts such as columns, autofit, and custom line spacing are skipped. An empty warning list does not prove that text fits.

Review warnings in the rendered images. Margin departures and overlaps can be intentional for backgrounds, full-bleed imagery, or layering; do not move them blindly. `release` removes assignments and relationships, not deck-wide margin checks or overlap/text-fit warnings. Render every slide and inspect text clipping, optical alignment, and reading order before reporting completion.

## Transactions and validation

Mutations are committed only after validation and package reopen checks. A failed operation leaves the original deck intact. Treat returned IDs and post-state as authoritative. Do not retry a failed mutation blindly: inspect the error and current deck state first.

## Tool payloads

Use these exact argument shapes. Slide indexes are one-based. Geometry values are inches.

- `deck_inspect`: `{}` for the whole deck, or `{"path":"/slide[2]"}` for a specific handler path. Results include slide size and shape geometry in inches.
- `slide_create`: `{}`.
- `slide_duplicate` and `slide_delete`: `{"index":2}`.
- `slide_reorder`: `{"from":4,"to":2}`.
- `text_add`: `{"slide":2,"text":"Label","x":1.0,"y":1.0,"width":3.0,"height":0.6,"font_size":24,"color":"#F4F7FA","font_family":"Arial","bold":true,"alignment":"left"}`. Set the foreground explicitly against the actual slide or shape background, especially on dark slides. Optional `bold` and `italic` are booleans; `alignment` is `left`, `center`, `right`, or `justify`. Choose a deliberate title/body/caption size hierarchy instead of leaving all text at its default size.
- `image_add`: `{"slide":2,"path":"/absolute/image.png","x":1.0,"y":1.0,"width":4.0,"height":3.0}`.
- `shape_add`: `{"slide":2,"kind":"hexagon","x":1.0,"y":1.0,"width":4.0,"height":1.0,"fill":"#336699"}`. Supported kinds are `rectangle`, `ellipse`, `hexagon`, `line`, and `connector`.
- Colors accept either `#RRGGBB` or `RRGGBB`; tools normalize them before writing OOXML.
- In text values, either an actual newline or `\\n` creates a line break.
Batch independent edits of the same kind with `edits`. The whole batch is atomic and advances the deck generation once:

```json
{"edits":[
  {"slide":1,"text":"Title","x":0.8,"y":0.5,"width":5.5,"height":0.7},
  {"slide":2,"text":"Summary","x":0.8,"y":0.5,"width":5.5,"height":0.7}
]}
```

Single-edit payloads remain valid. A batch may contain up to 100 edits. If any edit fails, none are committed.

- `element_update`: `{"id":"<stable ID>","properties":{"text":"Replacement text","font_size":"24","color":"#F4F7FA","font_family":"Arial","bold":"true"}}`. Put every changed value inside `properties`; update values are strings. Do not send `path` to this tool.
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
