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
