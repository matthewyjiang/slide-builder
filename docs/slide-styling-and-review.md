# Slide styling and review

The agent can set text color, font family, size, weight, and alignment when adding
text. Dark slides need an explicit contrasting text color; changing a background
does not automatically recolor existing text.
Text formatting currently requires the standard `p:` and `a:` XML namespace
prefixes; imported files using alternative prefixes return an explicit error.

When the agent calls `render_deck`, the rendered slide images are sent back to the
model automatically for visual review. Use an image-capable model for this workflow.
You can also attach the active slide with `Ctrl+V` to ask about a specific layout.
The preview is an HTML-based rendering, so check the final deck in PowerPoint when
exact Office rendering matters.

## Review before completion

The core agent prompt requires a layout audit and rendered review before the agent
reports layout work as finished. This applies to new content and to edits of text,
geometry, or styles. The agent must inspect text fit, unintended overlaps, peer
spacing, alignment, margins, and density, fix defects, and repeat the checks after
repairs. A successful tool call alone is not a visual review.

The agent must report unresolved defects or missing visual evidence rather than
claim the deck is finished. This is an agent instruction, not a runtime completion
lock or a guarantee of PowerPoint rendering fidelity. Automatic checks provide
partial evidence; the agent still needs to inspect the images.

## Posters and single-slide documents

Poster guidance keeps the requested canvas and slide count. The agent establishes
columns, section regions, and shared gutters before placing content, then uses
native layout operations for aligned edges and equal gaps between peer components.
Spacing within a section can be tighter than spacing between sections.

The agent must inspect both the full poster and dense sections at readable detail.
It should rebalance regions and shorten wording without losing meaning before
shrinking type. It must not silently discard required content or move it to notes
or extra slides. If the preview is too small to judge text fit, the agent must
obtain a readable view with available tools or report the review limitation.

For the audit's supported checks and limitations, see `docs/slide-layout.md`.

The rendered poster test builds a broken single-slide fixture with intersecting
text boxes, uneven gutters, and overflowing text, then repairs it with shared
regions. It checks rendered text pixels, not just box coordinates. On a host with
a qualified native Obscura sandbox, retain the before/after images and audit
reports with:

```sh
SLIDE_BUILDER_TEST_ARTIFACTS=/tmp/slide-builder-poster-review \
  cargo test --locked -j 12 --test poster_layout_render -- --ignored --nocapture
```

This tests the deck tools and renderer, not a model's compliance with the review
instructions.

For custom SVG illustrations, see `docs/assets.md`. Asset tools attach isolated
previews and validate a static SVG subset. Place by ID, then use `render_deck`
to review the asset in context; mechanical checks do not establish visual quality.
