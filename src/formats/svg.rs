use anyhow::Result;
use crate::report::StripReport;

/// Strip <metadata> and <rdf:RDF> elements from SVG using a simple byte scanner.
/// Handles well-formed SVG; does not attempt full XML parsing to keep it small.
pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    // Lossy: a non-UTF-8 byte becomes U+FFFD instead of failing the whole
    // file - the scanner only cares about ASCII tag structure.
    let text_owned = String::from_utf8_lossy(data);
    let text = text_owned.as_ref();
    let mut report = StripReport::default();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while !rest.is_empty() {
        // Look for an opening tag we want to strip
        if let Some((before, tag_name, after, self_closing)) = find_strip_open(rest) {
            out.push_str(before);
            if self_closing {
                // <metadata/> - the whole element is the opening tag itself.
                report.add(format!("SVG <{}> element", tag_name));
                rest = after;
            } else if let Some(close_end) = find_close(after, tag_name) {
                // Report the RDF property names inside the removed span
                // (e.g. dc:title, dcterms:created).
                report.add_fields(
                    format!("SVG <{}> element", tag_name),
                    xml_element_names(&after[..close_end]),
                );
                rest = &after[close_end..];
            } else {
                // Malformed - no close tag found, keep the original
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
/// Returns (before, tag_name, rest_after_tag_close_angle, is_self_closing).
fn find_strip_open(text: &str) -> Option<(&str, &str, &str, bool)> {
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
    let open_end_rel = text[pos..].find('>')?;
    let close_angle = pos + open_end_rel + 1;
    // Self-closing if the last character before '>' is '/' (<metadata/>).
    let self_closing = text[pos + 1..pos + open_end_rel].ends_with('/');
    Some((&text[..pos], tag, &text[close_angle..], self_closing))
}

/// Qualified element names (prefix:name) found in an XML fragment, e.g. the
/// RDF properties inside a stripped <metadata> block. Structural rdf:* tags
/// are excluded; names are deduplicated in order of first appearance.
fn xml_element_names(xml: &str) -> Vec<String> {
    let mut names = Vec::new();
    let bytes = xml.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        // Skip comments and processing instructions.
        if bytes.get(i + 1) == Some(&b'!') || bytes.get(i + 1) == Some(&b'?') {
            match xml[i..].find('>') {
                Some(end) => i += end + 1,
                None => break,
            }
            continue;
        }
        // Read the tag name (up to whitespace, '>' or '/').
        let start = i + 1;
        let mut j = start;
        while j < bytes.len()
            && !bytes[j].is_ascii_whitespace()
            && bytes[j] != b'>'
            && bytes[j] != b'/'
        {
            j += 1;
        }
        if j > start {
            let name = &xml[start..j];
            if name.contains(':')
                && !name.starts_with("rdf:")
                && !names.iter().any(|existing| existing == name)
            {
                names.push(name.to_string());
            }
        }
        i = j.max(i + 1);
    }
    names
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
