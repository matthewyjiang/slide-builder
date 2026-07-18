---
name: slide-builder
description: A focused terminal studio for creating and refining native PowerPoint decks.
colors:
  terminal-foreground: "terminal:foreground"
  terminal-background: "terminal:background"
  signal: "ansi:cyan"
  neutral: "ansi:gray"
  success: "ansi:green"
  warning: "ansi:yellow"
  danger: "ansi:red"
  signal-soft: "blend(terminal:background, ansi:cyan, 18%)"
  neutral-soft: "blend(terminal:background, ansi:gray, 10%)"
  success-soft: "blend(terminal:background, ansi:green, 16%)"
  warning-soft: "blend(terminal:background, ansi:yellow, 16%)"
  danger-soft: "blend(terminal:background, ansi:red, 16%)"
typography:
  title:
    fontFamily: "terminal monospace"
    fontWeight: 700
  body:
    fontFamily: "terminal monospace"
    fontWeight: 400
  label:
    fontFamily: "terminal monospace"
    fontWeight: 700
rounded:
  none: "0"
spacing:
  cell: "1ch"
  inline: "2ch"
  row: "1lh"
components:
  panel-title:
    textColor: "{colors.terminal-foreground}"
    typography: "{typography.title}"
  conversation-user:
    backgroundColor: "{colors.neutral-soft}"
    textColor: "{colors.terminal-foreground}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "1lh 1ch"
  conversation-system:
    backgroundColor: "{colors.warning-soft}"
    textColor: "{colors.terminal-foreground}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "1lh 1ch"
  tool-proposed:
    backgroundColor: "{colors.neutral-soft}"
    textColor: "{colors.terminal-foreground}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "1lh 1ch"
  tool-running:
    backgroundColor: "{colors.warning-soft}"
    textColor: "{colors.terminal-foreground}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "1lh 1ch"
  tool-succeeded:
    backgroundColor: "{colors.success-soft}"
    textColor: "{colors.terminal-foreground}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "1lh 1ch"
  tool-failed:
    backgroundColor: "{colors.danger-soft}"
    textColor: "{colors.terminal-foreground}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "1lh 1ch"
  slide-active:
    backgroundColor: "{colors.signal-soft}"
    textColor: "{colors.terminal-foreground}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
  keycap:
    backgroundColor: "{colors.neutral-soft}"
    textColor: "{colors.terminal-foreground}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "0 1ch"
---

# Design System: slide-builder

## Overview

**Creative North Star: "The Focused Studio"**

Slide-builder is a content-first editorial tool compressed into a professional terminal workspace. The deck and the work required to improve it remain central; interface chrome exists only to orient, expose state, and make the next action obvious. Density is welcome when it shortens the path between intent and a presentation-quality result.

The system is understated, technical, and focused. It borrows Rho's terminal-native conversation flow, Linear's disciplined hierarchy and restrained state color, and Notion's calm content-first posture. It explicitly rejects noisy cyberpunk styling, generic chatbot bubbles, repeated speaker labels, card-heavy SaaS composition, cute assistant behavior, and undifferentiated walls of terminal text.

**Key Characteristics:**

- Terminal-native density with clear panel ownership.
- Flat geometry organized by separators, spacing, and tonal surfaces.
- One restrained ANSI signal role reserved for focus and selection.
- Semantic state colors paired with glyphs and plain-language status text.
- Responsive layouts that remain useful in narrow terminals.

## Colors

The Focused Studio palette inherits the terminal's foreground, background, and ANSI color table. The application queries those values at startup, uses ANSI cyan for active intent, gray for neutral grouping, and green, yellow, and red for operational state, then blends soft surfaces toward the resolved ANSI colors. The frontmatter is the normative semantic token source and mirrors `src/tui/theme.rs`.

### Primary

- **Signal** (`signal`): The terminal's ANSI cyan role for focus, current selection, and primary interaction cues. Its rarity gives it authority.
- **Soft Signal** (`signal-soft`): The terminal background blended 18% toward ANSI cyan for selected rows and restrained active surfaces.

### Secondary

- **Success / Success Soft:** ANSI green and a 16% background blend for completed operations, paired with a check glyph and explicit success copy.
- **Warning / Warning Soft:** ANSI yellow and a 16% background blend for running activity and system notices, paired with a progress glyph or notice content.
- **Danger / Danger Soft:** ANSI red and a 16% background blend for failed operations, paired with a failure glyph and error text.

### Neutral

- **Terminal Background:** The user's configured workspace foundation.
- **Neutral Soft:** The terminal background blended 10% toward ANSI gray for user messages, proposed tools, keycaps, and low-elevation grouping.
- **Terminal Foreground:** The user's configured primary readable content color.
- **ANSI Gray:** Secondary copy, inactive controls, metadata, separators, and supporting detail.

**The Signal Rule.** The ANSI cyan signal role marks active intent and selection only. It is not decoration.

**The Redundancy Rule.** Success, warning, and danger colors must always be reinforced by a glyph, status word, or both.

## Typography

**Display Font:** The user's configured terminal monospace
**Body Font:** The user's configured terminal monospace
**Label/Mono Font:** The user's configured terminal monospace

**Character:** Typography inherits the terminal rather than imposing a bundled font. Hierarchy comes from weight, color, spacing, and placement so the interface remains native to each user's environment.

### Hierarchy

- **Display:** Not used. The product avoids oversized display typography inside the workspace.
- **Headline:** Bold terminal text for modal titles and major temporary surfaces.
- **Title:** Bold terminal text for persistent panel titles such as Conversation, Preview, Slides, and Prompt.
- **Body:** Regular terminal text for messages, tool detail, help, and deck content. Prose wraps to the available panel width.
- **Label:** Bold terminal text for statuses, selected items, keycaps, and concise controls.

**The Terminal Type Rule.** Do not simulate visual hierarchy with ASCII art, oversized letterforms, or decorative type. Use concise copy, weight, spacing, and color.

## Elevation

The system is flat with restrained tonal layers. It does not use shadows. Depth comes from the base surface, raised neutral blocks, semantic soft backgrounds, one-cell separators, and bordered overlays. Full borders are reserved for modals, presentation frames, copy feedback, and other genuinely overlaid surfaces.

Persistent workspace panels use separator lines rather than card containers. Conversation and tool blocks use full-width backgrounds because their grouping is semantic, not decorative. Modal overlays may clear the content beneath them and use a square one-cell border to establish temporary focus.

**The Flat Studio Rule.** Prefer spacing, separators, and tonal contrast over nested borders or decorative containers.

## Components

### Workspace Shell

The wide layout gives 56% of the body to Conversation and 44% to the Preview and Slides stack. Compact terminals stack Conversation, Preview, and Slides vertically above the prompt. One-cell separators define ownership without turning every panel into a card.

### Conversation Entries

Every message is width-aware and padded by one terminal column and one row. User messages use Neutral Soft, assistant messages remain on the terminal background, and system notices use Warning Soft. Roles are distinguished by surface treatment and context rather than repeated speaker labels.

### Tool Activity

Tool entries use the same full-width padded block structure as messages. Proposed, running, succeeded, and failed states each combine a semantic background, contrasting foreground, distinct glyph, and plain-language verb. Detail text remains secondary but must retain accessible contrast on the block surface.

### Slide List

The active slide uses Soft Signal across the row, bold terminal foreground text, and a `›` highlight symbol. Hover uses Neutral Soft. Inactive rows use ANSI gray. Selection must remain obvious without relying on color alone.

### Prompt Composer

The composer is anchored by top and bottom separators and grows with wrapped or multiline input. Its title shares the persistent panel-title treatment. Slash suggestions appear immediately above it as a temporary bordered surface rather than displacing the main workspace.

### Keycaps and Status

Keycaps use Neutral Soft, bold contrasting text, and one-column horizontal padding. Status surfaces pair concise labels with semantic ANSI color and remain visually subordinate to the deck and conversation.

### Modals

Menus, help, approval, configuration, and picker surfaces use square one-cell borders, a clear title, and compact internal spacing. Selection, help text, and validation status follow the same accent, muted, warning, and danger vocabulary as the workspace.

## Do's and Don'ts

### Do:

- **Do** keep the deck, conversation, and current task more visually prominent than application chrome.
- **Do** use the ANSI cyan signal role only for focus, active selection, and primary interaction cues.
- **Do** pair every semantic color with a glyph, status word, or explicit text explanation.
- **Do** derive foregrounds, backgrounds, and semantic colors from the terminal palette rather than hardcoding RGB values.
- **Do** preserve complete keyboard operation and visible focus or selection state.
- **Do** test layouts in narrow terminals and verify text remains readable in limited-color environments.
- **Do** use full-width tonal blocks when they clarify conversation or tool ownership.

### Don't:

- **Don't** introduce noisy cyberpunk or hacker-terminal styling.
- **Don't** use generic chatbot bubbles or repeated speaker labels.
- **Don't** turn the workspace into a card-heavy SaaS dashboard.
- **Don't** give the assistant a cute or overly playful visual personality.
- **Don't** allow the conversation to become a dense wall of undifferentiated terminal text.
- **Don't** use color as the only carrier of status or meaning.
- **Don't** hardcode RGB colors or assume a dark terminal background.
- **Don't** add shadows, rounded cards, decorative gradients, or accent color without a functional role.
