---
name: slide-design
description: "Use when composing or reviewing slide layouts: placing and sizing elements, establishing hierarchy, choosing a form for stats, comparisons, processes, quotes, or data, fixing slides that look off or generated, and checking rendered slides. This is the design-judgment layer, separate from the generator that emits the file and the design package that supplies palette, typography, spacing, and motif. Do not use it to operate the file format or invent brand styling."
---
# Slide design

This skill governs how design tokens are arranged on a slide so the message reads clearly and the result looks deliberate. The generator handles the file format. The active design package supplies the visual language.

## Use the design package

When a design package is active, `DESIGN.md` is the primary visual contract. Follow it before generic guidance here. The system prompt includes its full text and lists supplementary package files that can be read on demand.

Read `TEMPLATE-REFERENCE.md` only when listed and needed for layout names, placeholders, examples, or constraints. If package templates exist, the user must explicitly choose between a blank deck and each available template. Never silently select `template.pptx`.

Use the package's palette, typefaces, spacing scale, grid, and motif. Do not substitute generic preferences or invent brand claims, logos, colors, or proprietary styling. If no package is active, use restrained defaults.

## One message per slide

Before placing anything, answer: **what single thing should a viewer take away in three seconds?** Make that the focal point and give it the most visual weight. Everything else supports it or gets cut. Two co-equal messages require two slides; no clear message means the slide should not exist.

Every slide also needs a visual anchor that makes the message stick: an image, chart, diagram, icon set, large stat, or strong typographic statement. A title plus prose is usually a statement slide waiting to be simplified or content that belongs on a neighboring slide.

## Make importance visible

A viewer should be able to rank elements without reading them. Use these levers together:

1. **Size:** use decisive steps between title, section head, body, and caption. Slight differences look accidental.
2. **Weight and color:** bold and high contrast advance; regular and muted recede. Use the package accent surgically, usually for one focal element.
3. **Position:** in left-to-right layouts, account for the natural top-left entry and Z or F reading path.
4. **Whitespace:** isolation raises importance. Empty space is a hierarchy tool, not waste.

When these levers agree, the slide reads instantly. When they compete, it feels muddy.

## Structure before decoration

- **Grid:** establish consistent columns, margins, and gutters across the deck. Snap shared edges exactly; near-alignment reads as sloppiness.
- **Alignment:** left-align body text and lists. Center only short titles or single-line callouts, never paragraphs. Preserve alignment relationships across slides.
- **Grouping:** place related elements close together and separate unrelated groups with space. Try proximity before boxes, rules, or dividers.
- **Whitespace:** do not fill the surface. If a slide feels full, cut roughly a third of the content and enlarge what remains. Reuse gaps from the package spacing scale instead of inventing arbitrary distances.
- **Margins:** keep content and footers inside the package's breathing zone, uniformly across every slide.

## Match form to content

Do not default to title and bullets. Choose the form that exposes the content's structure:

- **One important number:** a large isolated stat with a short label.
- **Two alternatives:** side-by-side columns with parallel structure and aligned differences.
- **Sequence or process:** a directional flow of numbered steps.
- **Relationships among parts:** a hierarchy, cycle, map, or other diagram.
- **Quantitative comparison:** a chart styled with package tokens, with the conclusion stated in the title.
- **Statement or quote:** a near-empty or full-bleed composition with one large line.
- **Parallel items:** an evenly aligned grid whose cards share one internal structure.

Vary content layouts to create rhythm, but keep the package grid, type scale, spacing, and motif stable. Repeat title and section-break treatments so the audience can locate itself in the narrative. Carry the package motif through the deck rather than adding one-off flourishes.

## Write for presentation distance

Slides are not documents. Use headlines and short supporting phrases, not paragraphs to be read aloud. Cut sentences to phrases and phrases to keywords where meaning survives. A multi-line bullet usually belongs in the speaker's notes. Fewer, larger words improve readability, retention, and visual calm.

## Avoid generated-looking patterns

- Thin accent lines or underlines beneath titles.
- Decorative header, footer, edge, or card stripes.
- The same layout on every slide.
- Centered body text.
- Weak size contrast that leaves everything equally important.
- Equal visual weight for every color instead of one dominant tone and a focused accent.
- Boxes around groups that spacing could express.
- Content packed to the edges.

Use hierarchy and whitespace before decoration. If a block needs separation, prefer spacing or a subtle package-approved background treatment over an ornamental rule.

## Review rendered slides

Render and inspect every slide, not only the active one. Look fresh at the output rather than trusting the intended layout:

- **Focal point:** when squinting, does one element dominate?
- **Hierarchy:** can elements be ranked without reading them?
- **Overflow:** is any text clipped, wrapped badly, or outside its container? Check this first.
- **Alignment:** do shared edges, card tops, image captions, and baselines line up exactly?
- **Spacing:** are gaps consistent, with no collisions, cramped areas, or accidental voids?
- **Margins:** do all elements, including footers, clear the edge breathing zone?
- **Contrast:** is every word, icon, and data mark legible against its background?
- **Tells:** remove accent rules, edge stripes, centered paragraphs, and unnecessary boxes.
- **Deck fit:** does the slide share the package grid, type scale, spacing, and motif with its neighbors?

Fix real defects, render again, and recheck affected slides. Stop when the message is clear, the system is coherent, and another change would not materially help the audience.
