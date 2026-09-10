---
name: slide-design
description: "Use when composing or reviewing presentation slides, posters, or single-slide documents: placing and sizing elements, balancing density, establishing hierarchy, choosing a form for stats, comparisons, processes, quotes, or data, fixing overflow, overlaps, uneven spacing, or generated-looking layouts, and checking rendered slides. This is the design-judgment layer, separate from the generator that emits the file and the design package that supplies palette, typography, spacing, and motif. Do not use it to operate the file format or invent brand styling."
---
# Slide design

This skill governs how design tokens are arranged on a slide so the message reads clearly and the result looks deliberate. The generator handles the file format. The active design package supplies the visual language.

## Use the design package

When a design package is active, `DESIGN.md` is the primary visual contract. Follow it before generic guidance here. The system prompt includes its full text and lists supplementary package files that can be read on demand.

Read `TEMPLATE-REFERENCE.md` only when listed and needed for layout names, placeholders, examples, or constraints. If package templates exist, the user must explicitly choose between a blank deck and each available template. Never silently select `template.pptx`.

Use the package's palette, typefaces, spacing scale, grid, and motif. Do not substitute generic preferences or invent brand claims, logos, colors, or proprietary styling. If no package is active, use restrained defaults.

## Establish reusable geometry and type

Inspect the deck's shared layout contract before composing. Reuse its named regions and text styles instead of recreating coordinates and formatting for each slide. If none exists, resolve the package's rules into a contract with `deck_layout_set`. In an existing deck, preserve observed conventions unless the user asks for a redesign. For a new deck without a package, choose a restrained system, render a representative content slide, and adjust it before extending the deck.

Use `elements_layout` for exact shared edges, equal gaps, matching dimensions, and placement inside named regions. The model decides which elements belong together; the tool computes their coordinates. Apply named text styles to make title, evidence, and source roles consistent across slides. A contract is a reusable starting point, not a requirement that every slide use the same composition.

## Posters and single-slide documents

Treat a poster as a document read by section within one canvas. Keep the requested slide count and dimensions. The presentation advice below about extra slides, speaker notes, and short phrases does not override a poster's required content.

1. Inspect the actual canvas size. Establish outer margins, columns, section regions, and shared gutters in the layout contract before adding content. Allocate room for headings, body text, figures, captions, and sources within each section.
2. Use `elements_layout` to align peer components and distribute them with equal gaps. Use tighter gaps within a group and consistent larger gaps between peer sections. Do not distribute headings and their body text as unrelated peers. Saving a gutter alone does not apply it to elements.
3. Fit content to its region. After text edits or changes to width, font, or size, check wrapping and text height again. Text-box bounds are not proof that the text fits. Reserve space between the rendered text and the next component.
4. Balance density across columns. Reallocate space or shorten wording while preserving claims and qualifiers before reducing type size. Enlarge undersized content or rebalance regions when the canvas has accidental empty areas. Keep deliberate whitespace that supports grouping. Do not silently remove required content, move it to notes, add slides, or change the canvas to make it fit.
5. Audit the saved layout, render the poster, and inspect both the whole canvas and dense sections at readable detail. Fix text overflow, unintended overlaps, uneven peer gutters, and crowded or underused regions before reporting completion. If the available image is too small to judge text fit, obtain a readable view with available tools or report that limitation.

## One message per presentation slide

For presentation slides, before placing anything, answer: **what single thing should a viewer take away in three seconds?** Make that the focal point and give it the most visual weight. Everything else supports it or gets cut. Two co-equal messages require two slides; no clear message means the slide should not exist.

Every slide also needs a visual anchor that makes the message stick: an image, chart, diagram, icon set, large stat, or strong typographic statement. A title plus prose is usually a statement slide waiting to be simplified or content that belongs on a neighboring slide.

Before placing content, identify the takeaway, primary visual, supporting evidence, and secondary context. Assign visual roles in that order. Sources and caveats must remain readable, but should not compete with the claim they support.

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
- **Whitespace:** do not fill the surface. If a slide feels full, shorten wording or reorganize content while preserving required meaning, then enlarge what remains. If it feels empty, enlarge undersized content or rebalance regions instead of adding filler. Reuse gaps from the package spacing scale instead of inventing arbitrary distances.
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

Reuse relationships within each composition:

- **Comparison:** align column headings and corresponding evidence rows. Use matching widths and the same internal spacing. Place the conclusion outside the comparison group.
- **Chart with takeaway:** give the chart the primary evidence region. Align its caption and source with the chart edge; keep explanatory text secondary.
- **Image with explanation:** preserve image proportions and align the explanation with the image or caption. Do not stretch an image merely to fill a region.
- **Key metric:** make the number and its meaning one group. Keep contextual comparisons and sources subordinate.
- **Process:** distribute steps evenly, align their labels, and reserve room for connectors instead of adding them over text.
- **Statement:** use one clear typographic focal point and leave supporting detail off the slide when it belongs in notes.

Repeated cards are appropriate only for genuinely parallel items. Do not force unrelated content into equal boxes to make the layout look orderly.

## Write for presentation distance

Slides are not documents. Use headlines and short supporting phrases, not paragraphs to be read aloud. Cut sentences to phrases and phrases to keywords where meaning survives. A multi-line bullet usually belongs in the speaker's notes. Fewer, larger words improve readability, retention, and visual calm.

When content does not fit, shorten it, move detail to notes, or split the slide before reducing the text size. Preserve the important claims and qualifiers. Do not silently delete meaning to satisfy a layout.

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

Review is required before reporting completion, including the first result shown as finished. Run `deck_layout_audit` and read its findings and coverage limitations, then render and inspect every slide, not only the active one. Compare neighboring slides for title placement, type roles, and density before reviewing individual defects. For a single-slide poster, compare peer sections and columns. A clean mechanical report does not prove text fit or good hierarchy. Look at the output rather than trusting the intended layout:

- **Focal point:** when squinting, does one element dominate?
- **Hierarchy:** can elements be ranked without reading them?
- **Overflow:** is any text clipped, wrapped badly, or outside its container? Check this first.
- **Alignment:** do shared edges, card tops, image captions, and baselines line up exactly?
- **Spacing:** are gaps consistent, with no collisions, cramped areas, or accidental voids?
- **Margins:** do all elements, including footers, clear the edge breathing zone?
- **Contrast:** is every word, icon, and data mark legible against its background?
- **Tells:** remove accent rules, edge stripes, centered paragraphs, and unnecessary boxes.
- **Deck fit:** does the slide share the package grid, type scale, spacing, and motif with its neighbors?

Fix real defects, repeat the audit, render again, and recheck affected slides before reporting completion. Do not wait for the user to point out clipping, overlaps, or uneven gaps. Preserve intentional layering; inspect warnings in context rather than removing backgrounds or releasing valid spacing rules to clear the report. Stop when the message is clear and the checks reveal no unresolved defects. If visual review is unavailable or a defect remains, state the limitation rather than claiming the layout is finished.
