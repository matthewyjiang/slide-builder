//! Grapheme-safe text measurement and wrapping shared by conversation rendering.
use ratatui::{style::Style, text::Span};
use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(super) fn display_width(text: &str) -> usize {
    text.split(char::is_control)
        .map(UnicodeWidthStr::width)
        .sum()
}

pub(super) fn truncate_to_display_width(text: &str, max_width: usize) -> Cow<'_, str> {
    if display_width(text) <= max_width {
        return Cow::Borrowed(text);
    }
    let mut end = 0;
    let mut width = 0;
    for (index, grapheme) in text.grapheme_indices(true) {
        let grapheme_width = display_width(grapheme);
        if width + grapheme_width > max_width {
            break;
        }
        width += grapheme_width;
        end = index + grapheme.len();
    }
    Cow::Borrowed(&text[..end])
}

/// Wrap at whitespace without allowing the first break to strand a semantic prefix.
///
/// `protected_prefix_end` is a byte offset whose preceding whitespace cannot be
/// used as the first wrap point. If the following token overflows, the first
/// line is filled to `width` instead.
pub(super) fn wrap_line_at_whitespace_ranges_with_protected_prefix(
    line: &str,
    width: usize,
    protected_prefix_end: usize,
) -> Vec<std::ops::Range<usize>> {
    let width = width.max(1);
    if line.is_empty() {
        return std::iter::once(0..0).collect();
    }

    let mut ranges = Vec::new();
    let mut start = 0;
    while start < line.len() {
        let mut count = 0usize;
        let mut last_fitting_split = None;
        let mut whitespace_break = None;
        let mut saw_non_whitespace = false;
        let mut overflow = false;
        let mut prefer_width_split = false;

        for (relative_index, grapheme) in line[start..].grapheme_indices(true) {
            let grapheme_width = display_width(grapheme);
            let whitespace = grapheme.chars().all(char::is_whitespace);
            let next = start + relative_index + grapheme.len();
            if count == 0 && grapheme_width > width {
                last_fitting_split = Some(next);
                overflow = true;
                break;
            }
            if count > 0 && count + grapheme_width > width {
                overflow = true;
                prefer_width_split = whitespace;
                break;
            }

            count += grapheme_width;
            last_fitting_split = Some(next);
            if whitespace {
                if saw_non_whitespace {
                    whitespace_break = Some(next);
                }
            } else {
                saw_non_whitespace = true;
            }
        }

        if !overflow {
            ranges.push(start..line.len());
            break;
        }

        let split = if prefer_width_split
            || (start == 0 && whitespace_break.is_some_and(|split| split <= protected_prefix_end))
        {
            last_fitting_split.expect("overflow requires a fitting split")
        } else {
            whitespace_break
                .filter(|split| *split > start)
                .unwrap_or_else(|| last_fitting_split.expect("overflow requires a fitting split"))
        };
        ranges.push(start..split);
        start = split;
    }

    ranges
}

/// Collapse break-boundary whitespace from covering soft-wrap ranges for display.
///
/// After a range that contained non-whitespace, leading whitespace on the next
/// range is break padding and is dropped so the continuation is not indented.
/// Pure whitespace segments keep their spaces so blank padding still wraps.
pub(super) fn soft_wrap_visible_ranges<'a>(
    line: &'a str,
    ranges: impl IntoIterator<Item = std::ops::Range<usize>> + 'a,
) -> impl Iterator<Item = std::ops::Range<usize>> + 'a {
    let mut prev_had_non_whitespace = false;
    ranges.into_iter().filter_map(move |range| {
        let end = range.end;
        let mut start = range.start;
        if prev_had_non_whitespace {
            while start < end {
                let grapheme = line[start..].graphemes(true).next().expect("start < end");
                if !grapheme.chars().all(char::is_whitespace) {
                    break;
                }
                start += grapheme.len();
            }
            if start >= end {
                return None;
            }
        }
        prev_had_non_whitespace = line[start..end].chars().any(|ch| !ch.is_whitespace());
        Some(start..end)
    })
}

/// Hard-wrap `text` into display-width columns as byte ranges into `text`.
///
/// Empty input yields one empty range. Grapheme clusters are never split. A
/// chunk that exactly fills `width` breaks after it.
pub(super) fn hard_wrap_ranges(text: &str, width: usize) -> Vec<std::ops::Range<usize>> {
    let width = width.max(1);
    if text.is_empty() {
        return vec![std::ops::Range { start: 0, end: 0 }];
    }
    let mut ranges = Vec::new();
    let mut chunk_start = 0usize;
    let mut offset = 0usize;
    let mut current_width = 0usize;
    for grapheme in text.graphemes(true) {
        let grapheme_width = display_width(grapheme);
        if current_width > 0 && current_width + grapheme_width > width {
            ranges.push(chunk_start..offset);
            chunk_start = offset;
            current_width = 0;
        }
        offset += grapheme.len();
        current_width += grapheme_width;
        if current_width >= width {
            ranges.push(chunk_start..offset);
            chunk_start = offset;
            current_width = 0;
        }
    }
    if chunk_start < text.len() {
        ranges.push(chunk_start..text.len());
    }
    ranges
}

/// Hard-wrap a pre-styled line at display columns, preserving span styles.
///
/// `text` must be the concatenation of `spans` contents. Empty input yields one
/// empty span row using `empty_style`.
pub(super) fn hard_wrap_styled_spans(
    text: &str,
    spans: &[Span<'static>],
    width: usize,
    empty_style: Style,
) -> Vec<Vec<Span<'static>>> {
    let width = width.max(1);
    let mut span_index = 0;
    let mut span_start = 0;
    hard_wrap_ranges(text, width)
        .into_iter()
        .map(|range| {
            let mut chunk = Vec::new();
            while let Some(span) = spans.get(span_index) {
                if span_start >= range.end {
                    break;
                }
                let span_end = span_start + span.content.len();
                let from = range.start.saturating_sub(span_start);
                let to = (range.end - span_start).min(span.content.len());
                if from < to {
                    chunk.push(Span::styled(span.content[from..to].to_owned(), span.style));
                }
                if span_end > range.end {
                    break;
                }
                span_start = span_end;
                span_index += 1;
            }
            if chunk.is_empty() {
                vec![Span::styled(String::new(), empty_style)]
            } else {
                chunk
            }
        })
        .collect()
}

/// Slice concatenated spans by byte offsets into their joined UTF-8 text.
pub(super) fn slice_spans_by_bytes(
    spans: &[Span<'static>],
    start: usize,
    end: usize,
) -> Vec<Span<'static>> {
    if start >= end {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut offset = 0usize;
    for span in spans {
        let content = span.content.as_ref();
        let span_start = offset;
        let span_end = offset + content.len();
        offset = span_end;
        if span_end <= start || span_start >= end {
            continue;
        }
        let from = start.saturating_sub(span_start);
        let to = (end - span_start).min(content.len());
        if from >= to {
            continue;
        }
        // Ranges come from the concatenated UTF-8 text, so byte edges are char edges.
        out.push(Span::styled(content[from..to].to_string(), span.style));
    }
    out
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
