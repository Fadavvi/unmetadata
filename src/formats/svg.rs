use anyhow::Result;
use crate::report::StripReport;

/// Strip <metadata> and <rdf:RDF> elements from SVG using a simple byte scanner.
/// Handles well-formed SVG; does not attempt full XML parsing to keep it small.
pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let text = std::str::from_utf8(data)?;
    let mut report = StripReport::default();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while !rest.is_empty() {
        // Look for an opening tag we want to strip
        if let Some((before, tag_name, after)) = find_strip_open(rest) {
            out.push_str(before);
            if let Some(close_end) = find_close(after, tag_name) {
                report.add(format!("SVG <{}> element", tag_name));
                rest = &after[close_end..];
            } else {
                // Malformed — no close tag found, keep the original
                out.push_str(&format!("<{}", tag_name));
                rest = after;
            }
        } else {
            out.push_str(rest);
            break;
        }
    }

    Ok((out.into_bytes(), report))
}

/// Find the first strippable opening tag in `text`.
/// Returns (before, tag_name, rest_after_tag_close_angle).
fn find_strip_open(text: &str) -> Option<(&str, &str, &str)> {
    const STRIP_TAGS: &[&str] = &["metadata", "rdf:RDF"];

    let mut best: Option<(usize, &str)> = None;
    for &tag in STRIP_TAGS {
        let pattern = format!("<{}", tag);
        if let Some(pos) = text.find(&pattern) {
            // Make sure next char after tag name is '>' or ' ' or '/' or '\n'
            let after_name = &text[pos + pattern.len()..];
            let next = after_name.chars().next();
            if matches!(next, Some('>' | ' ' | '\t' | '\n' | '\r' | '/')) {
                best = match best {
                    None => Some((pos, tag)),
                    Some((best_pos, _)) if pos < best_pos => Some((pos, tag)),
                    other => other,
                };
            }
        }
    }
    let (pos, tag) = best?;
    let close_angle = text[pos..].find('>')? + pos + 1;
    Some((&text[..pos], tag, &text[close_angle..]))
}

/// Find the end of the closing tag for `tag_name`, handling nesting.
fn find_close(text: &str, tag_name: &str) -> Option<usize> {
    let open_pat = format!("<{}", tag_name);
    let close_pat = format!("</{}", tag_name);
    let mut depth = 1usize;
    let mut pos = 0;

    while pos < text.len() {
        if text[pos..].starts_with(&close_pat) {
            // find '>' after this
            let end = text[pos..].find('>')? + pos + 1;
            depth -= 1;
            if depth == 0 {
                return Some(end);
            }
            pos = end;
        } else if text[pos..].starts_with(&open_pat) {
            let end = text[pos..].find('>')? + pos + 1;
            // self-closing tags don't increase depth
            let tag_content = &text[pos + 1..end - 1];
            if !tag_content.ends_with('/') {
                depth += 1;
            }
            pos = end;
        } else {
            pos += 1;
        }
    }
    None
}
