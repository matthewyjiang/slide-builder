# Consistent layout and hierarchy

The agent can save deck-wide margins, gutters, named regions, and text styles
inside the PowerPoint file. These settings travel with the deck, rather than
depending on the conversation remaining in context. The agent derives them from
your design package or the existing slides; saving settings does not silently
reformat a deck.

Native layout operations calculate shared edges, equal gaps, matching dimensions,
and row or column placement. Named text styles keep headlines and supporting text
consistent. You can ask, for example, "Align the comparison columns, equalize their
gaps, and use the same headline style throughout."

`deck_layout_inspect` reads the settings, `deck_layout_set` replaces them, and
`elements_layout` applies them to stable element IDs. `deck_layout_audit` reports
changes to declared alignment, spacing, region placement, and text styles, alongside
bounds and margin findings. Later edits can be checked against the saved
relationships; an explicit `release` operation forgets relationships without
moving or restyling content. Saving a gutter does not apply it to shapes or declare
which gaps should match. The agent must use alignment, distribution, or region
placement operations to establish those relationships.

`deck_validate` separately checks the package. Neither tool replaces rendered
review for hierarchy, wrapping, text clipping, or image cropping.

## Overlap and text-fit warnings

The audit returns `issues` for bounds, margins, assigned styles, and declared
geometry rules. Its `valid` flag means only that this list is empty. Read
`warnings` as well, even when `valid` is true.

- `possible_overlap` identifies intersecting, unrotated element rectangles on the
  same slide. It reports the element IDs, rectangles, and the type of intersection.
  Text contained within another text box also gets a warning. Non-text background
  containment and touching edges do not trigger warnings.
- `possible_text_overflow` estimates vertical demand from declared font sizes and
  explicit paragraphs or line breaks, then compares it with the box's inner height.
  The report includes both heights in inches and the basis of the estimate. This
  is a warning, not a measurement of actual line height or rendered glyphs.

The audit does not infer which overlaps are intentional. Review warnings in the
rendered output before moving elements or changing text. Rotated shapes and
connectors are excluded from overlap warnings. Text-fit warnings do not measure
automatic wrapping, glyph widths, font substitution, columns, non-horizontal text,
rotation, autofit, or custom line spacing. A narrow box with a long wrapping
paragraph may therefore produce no warning even when it overflows. Missing
explicit text sizes can also leave text unmeasured.

The agent must still inspect the rendered slides and repair defects before
reporting completion. See `docs/slide-styling-and-review.md` for the poster workflow
and rendered test fixture.

## Preview proportions

Previews use the slide dimensions stored in the deck, including 4:3, square,
portrait, and custom sizes. The renderer fits the entire slide within the preview
resolution without stretching or cropping it. The PNG uses the slide's own aspect
ratio, rounded to whole pixels, rather than adding blank space to make it 16:9.
The terminal fits that image into the available preview area.

## Limitations

Region placement resizes elements; preserve image proportions unless stretching
is intentional. Full-bleed imagery can legitimately depart from content margins.
Layout operations currently support direct slide shapes and pictures, not grouped
elements or graphic frames. Geometry checks use transform rectangles rather than
rotated visual bounds; text checks compare explicitly applied styles rather than
theme-inherited appearance.
