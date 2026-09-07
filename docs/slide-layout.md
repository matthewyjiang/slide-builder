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
moving or restyling content. `deck_validate` separately checks the package. Neither
replaces rendered review for hierarchy, wrapping, text clipping, or image cropping.

## Limitations

Region placement resizes elements; preserve image proportions unless stretching
is intentional. Full-bleed imagery can legitimately depart from content margins.
Layout operations currently support direct slide shapes and pictures, not grouped
elements or graphic frames. Geometry checks use transform rectangles rather than
rotated visual bounds; text checks compare explicitly applied styles rather than
theme-inherited appearance.
