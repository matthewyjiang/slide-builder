# Managed SVG assets

Slide-builder can author custom SVG icons and conceptual illustrations through
native agent tools. No separate image provider, shell renderer, or network call
is needed. The model supplies SVG source; the application validates it, renders
a preview, stores the candidate, and embeds it by ID.
Use an image-capable model for preview review. If the model cannot inspect the
attached images, visual review remains incomplete.

This first version does not generate raster AI images. Keep charts, tables,
labels, and factual diagrams native and editable where possible. Use original
screenshots, measurements, and equipment photos rather than generated substitutes.

## Workflow

1. Call `asset_list` with `{}` to find reusable candidates for the active deck.
   Corrupt records or stray JSON files are reported as warnings without hiding
   the remaining candidates. Inspection and placement still reject a bad record.
2. Call `asset_create_svg` with a name, brief, and SVG source. The result includes
   an ID, source digest, validation findings, and an attached PNG preview.
3. Inspect that preview for defects and compliance with the brief. Use
   `asset_inspect` with `{"id":"<asset UUID>"}` to retrieve its source and preview
   later. For a revision, call `asset_create_svg` with `revises` set to the old ID.
   The original candidate remains unchanged.
4. Call `asset_place` with the ID, slide index, and target box in inches. The tool
   centers the full image inside the box without stretching or cropping it.
5. Call `render_deck`. Inspect the composed slide for contrast, legibility,
   hierarchy, crop, style consistency, and meaning. Review every use of an asset,
   including a previously reviewed asset placed on a different slide.

For example, the agent can create this simple marker:

```json
{
  "name": "conceptual rover marker",
  "brief": {
    "purpose": "Mark the rover in a conceptual mission illustration",
    "style": "Solid deck blue, transparent background, simple silhouette",
    "alt_text": "Conceptual rover silhouette"
  },
  "svg": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 60\"><rect x=\"20\" y=\"15\" width=\"60\" height=\"25\" fill=\"#1256ab\"/><circle cx=\"30\" cy=\"45\" r=\"10\" fill=\"#1256ab\"/><circle cx=\"70\" cy=\"45\" r=\"10\" fill=\"#1256ab\"/></svg>"
}
```

Then place the returned ID:

```json
{"id":"<asset UUID>","slide":1,"x":8,"y":2,"width":3,"height":3}
```

Names and briefs are descriptive metadata, not instructions that the validator
can prove. The style brief should specify the deck palette, stroke treatment,
composition, background treatment, and any approved references. The terminal
application's theme is not automatically the deck's visual style.

## What the gates establish

Hard failures prevent candidate creation or insertion:

- Malformed XML, unsupported elements or attributes, invalid numeric geometry,
  missing or nonpositive `viewBox`, and invalid path syntax.
- Scripts, event handlers, CSS, processing instructions, DTDs, foreign content,
  external references, embedded images, and text inside SVG.
- Artwork with no visible pixels in the rendered viewport.
- Corrupted asset records, source digest mismatches, invalid IDs, or redirected
  asset storage paths.
- Invalid placement geometry or an insertion outside the slide, checked inside
  the existing transactional deck mutation.

Artwork touching the preview boundary produces a possible-clipping warning.
This is advisory because full-bleed artwork may be intentional. A successful
validation reports its scope as static SVG and visible preview only. It does
not certify aesthetics, style compliance, factual accuracy, or target-application
rendering. There is no numerical beauty score or automated approval loop.

Use concrete revision feedback such as "the antenna disappears at this size;
simplify it and thicken its stroke." If review is incomplete, say so. Do not
repeatedly generate candidates without discussing an unresolved issue with the user.

## Supported SVG subset

Supported elements are `svg`, `g`, `path`, `rect`, `circle`, `ellipse`, `line`,
`polyline`, and `polygon`. Declare the SVG namespace with
`xmlns="http://www.w3.org/2000/svg"`. Use an explicit positive `viewBox`, solid
fills and strokes, numeric or pixel dimensions, and supported transforms. Either
provide both root `width` and `height`, or omit both and use the `viewBox` aspect
ratio. Use native slide text for labels.

Gradients, filters, masks, clipping paths, reuse through `use`, dashed strokes,
and group opacity below one are not supported. Per-paint `fill-opacity` and
`stroke-opacity` are supported. These restrictions keep rendering self-contained
and exclude expansion through references, filters, compositing layers, or tiny
dash patterns. Unsupported syntax fails with a diagnostic instead of being
silently removed.

The SVG source and serialized asset record each reuse the existing 32 MiB media
byte budget. The name and brief also use the existing 1,000,000-byte text budget.
Errors report the budget and requested size. Previews use the
application's default capture longest side, currently 1280 pixels. A square RGBA
preview therefore allocates 6,553,600 bytes, independent of SVG source dimensions.
The streaming nesting check allows depth 64, with the root at depth zero. This
was measured with nested transforms and a translucent shape in a Rust debug
build: depth 128 passed on a 2 MiB worker stack but overflowed on 1.5 MiB; depth
64 passed on 1 MiB. The check runs before the recursive XML/SVG parsers, so an
over-budget document returns a diagnostic instead of overflowing the stack.
These limits bound input size, raster allocation, and nesting, not render time.
Detailed paths and heavy overdraw can still be expensive. Rendering runs on a
blocking worker and does not have a separate CPU deadline in this version.

## Storage and export

Candidates live alongside the deck in `<deck filename>.assets/`, one immutable
JSON record per UUID. Each record contains the original SVG, brief, source
SHA-256, and optional revision parent. Previews are regenerated on inspection and
placement rather than trusted from a cache. Source digests detect accidental
edits; they are not cryptographic attestations of authorship.

Copy or rename the sidecar directory along with the deck to retain the candidate
library. Already placed assets remain embedded and do not need the sidecar to
render. Revising a candidate does not automatically replace its existing uses.
The returned stable picture ID can be used with normal element operations.

Placement embeds the SVG vector source with a PNG fallback using the Office SVG
extension. The HTML-based live preview uses the PNG fallback. Both media files
travel inside the PowerPoint file; nothing links to an external asset path.
These are picture objects, not native editable PowerPoint shapes. Check the
exported deck in the intended PowerPoint environment when exact Office fidelity
matters. Package validation and the HTML preview are not substitutes for that check.

Asset writes use the same permission policy as deck work. Plan mode blocks
candidate creation and placement while allowing listing and inspection.
