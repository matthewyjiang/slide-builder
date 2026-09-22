//! Markdown image parsing; decoding and display belong to the conversation UI.

/// An `![alt](path)` image reference found in assistant markdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MarkdownImageSource {
    pub(super) alt: String,
    pub(super) path: String,
}

/// Parses a line that consists only of an image reference, optionally padded
/// with whitespace. Inline images mixed into prose fall back to alt text.
pub(super) fn standalone_markdown_image(line: &str) -> Option<MarkdownImageSource> {
    let trimmed = line.trim();
    if !trimmed.starts_with("![") {
        return None;
    }
    let (image, range) = next_markdown_image(trimmed)?;
    (range == (0..trimmed.len())).then_some(image)
}

/// Parses the next `![alt](path)` span in `line`.
pub(super) fn next_markdown_image(
    line: &str,
) -> Option<(MarkdownImageSource, std::ops::Range<usize>)> {
    let start = line.find("![")?;
    let label_start = start + 2;
    let close_label = line[label_start..].find(']')? + label_start;
    let target_start = close_label + 2;
    if !line[close_label + 1..].starts_with('(') || target_start >= line.len() {
        return None;
    }

    let mut depth = 1usize;
    let mut escaped = false;
    let mut target_end = None;
    for (offset, ch) in line[target_start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    target_end = Some(target_start + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let target_end = target_end?;
    let alt = line[label_start..close_label].trim();
    let path = line[target_start..target_end].trim();
    (!path.is_empty()).then(|| {
        (
            MarkdownImageSource {
                alt: alt.to_string(),
                path: path.replace("\\(", "(").replace("\\)", ")"),
            },
            start..target_end + 1,
        )
    })
}
