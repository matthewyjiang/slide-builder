use super::*;

/// Byte end of the visible streaming prefix. Complete lines are always visible;
/// the open line waits at its first unfinished inline marker or fence fragment.
/// Wrapping does not affect visibility, and the caller retains the mutable block
/// from its opener rather than committing individual wrapped rows.
pub(in crate::tui) fn markdown_preview_end(text: &str) -> usize {
    let current_line_start = text.rfind('\n').map_or(0, |index| index + 1);
    let current_line = &text[current_line_start..];
    if current_line.is_empty() || starts_with_code_fence_fragment(current_line) {
        return current_line_start;
    }

    let mut active_fence = None;
    for line in text[..current_line_start].lines() {
        if active_fence.is_some_and(|fence| is_closing_fence(line, fence)) {
            active_fence = None;
        } else if active_fence.is_none() {
            active_fence = parse_opening_fence(line);
        }
    }

    let (stable_line, rendered_width) = if active_fence.is_some() {
        (current_line, display_width(current_line))
    } else {
        let stable_line = &current_line[..inline_markdown_stable_prefix_len(current_line)];
        (
            stable_line,
            display_width(&markdown_inline_text(stable_line)),
        )
    };
    if rendered_width > 0 {
        current_line_start + stable_line.len()
    } else {
        current_line_start
    }
}

fn starts_with_code_fence_fragment(line: &str) -> bool {
    let trimmed = line.trim_start();
    !trimmed.is_empty()
        && (trimmed.starts_with("```") || (trimmed.len() < 3 && "```".starts_with(trimmed)))
}

/// Returns the start of the trailing block that can still change as markdown is appended.
///
/// Markdown is line-oriented except for fenced code blocks, display math, and tables. Keeping
/// the final block mutable lets the history cache promote completed blocks and
/// re-render only this suffix as streaming text arrives.
pub(in crate::tui) fn incremental_markdown_tail_start(text: &str) -> usize {
    let mut line_offsets = Vec::new();
    let mut raw_lines = Vec::new();
    let mut offset = 0;
    for source_line in text.split_inclusive('\n') {
        let raw_line = source_line.strip_suffix('\n').unwrap_or(source_line);
        let raw_line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        line_offsets.push(offset);
        raw_lines.push(raw_line);
        offset += source_line.len();
    }
    if raw_lines.is_empty() {
        return 0;
    }

    let mut line_index = 0;
    let mut trailing_block_start = 0;
    while line_index < raw_lines.len() {
        trailing_block_start = line_offsets[line_index];
        if let Some(opening) = parse_opening_fence(raw_lines[line_index]) {
            line_index += 1;
            while line_index < raw_lines.len() {
                let closes_block = is_closing_fence(raw_lines[line_index], opening);
                line_index += 1;
                if closes_block {
                    break;
                }
            }
            continue;
        }
        match math::display_math_span(&raw_lines[line_index..]) {
            Some(math::DisplayMathSpan::Complete { line_count }) => {
                line_index += line_count;
                continue;
            }
            Some(math::DisplayMathSpan::Incomplete) => {
                line_index = raw_lines.len();
                continue;
            }
            None => {}
        }
        if let Some(consumed_lines) = table::markdown_table_line_count(&raw_lines[line_index..]) {
            line_index += consumed_lines;
            continue;
        }
        line_index += 1;
    }
    trailing_block_start
}
