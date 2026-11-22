use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::io::Cursor;
use lopdf::Document;
use crate::report::StripReport;

const INFO_FIELDS: &[(&[u8], &str)] = &[
    (b"Author", "Author"),
    (b"Creator", "Creator"),
    (b"Producer", "Producer"),
    (b"Subject", "Subject"),
    (b"Title", "Title"),
    (b"Keywords", "Keywords"),
    (b"CreationDate", "CreationDate"),
    (b"ModDate", "ModDate"),
    (b"Trapped", "Trapped"),
];

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut doc = load_with_recovery(data)?;
    let mut report = StripReport::default();

    // 1. Strip Info dictionary fields
    if let Ok(info_ref) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
        if let Ok(obj) = doc.get_object_mut(info_ref) {
            if let Ok(dict) = obj.as_dict_mut() {
                for (key, label) in INFO_FIELDS {
                    if dict.remove(*key).is_some() {
                        report.add(format!("PDF Info: {}", label));
                    }
                }
            }
        }
    }

    // 2. Find XMP metadata stream reference before mutably borrowing doc
    let root_id = doc.trailer.get(b"Root").and_then(|o| o.as_reference()).ok();

    let meta_id = root_id.and_then(|rid| {
        let obj = doc.get_object(rid).ok()?;
        let dict = obj.as_dict().ok()?;
        let meta = dict.get(b"Metadata").ok()?;
        meta.as_reference().ok()
    });

    // 3. Remove XMP stream and catalog reference
    if let Some(mid) = meta_id {
        if let Some(rid) = root_id {
            if let Ok(obj) = doc.get_object_mut(rid) {
                if let Ok(dict) = obj.as_dict_mut() {
                    dict.remove(b"Metadata");
                }
            }
        }
        doc.objects.remove(&mid);
        report.add("PDF XMP metadata stream");
    }

    let mut out = Cursor::new(Vec::new());
    doc.save_to(&mut out)?;
    Ok((out.into_inner(), report))
}

/// Load the document, falling back to a whole-file object scan when the xref
/// table is broken (common in the wild): rebuild a valid xref table + trailer
/// and append it as an incremental update.
fn load_with_recovery(data: &[u8]) -> Result<Document> {
    if let Ok(doc) = Document::load_mem(data) {
        return Ok(doc);
    }

    // Scan for "id gen obj" definitions across the whole file. Later
    // definitions win, matching incremental-update semantics.
    let mut offsets: BTreeMap<u32, usize> = BTreeMap::new();
    let mut pos = 0usize;
    while pos < data.len() {
        while pos < data.len() && !data[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos >= data.len() {
            break;
        }
        match parse_object_header(data, pos) {
            Some((id, after)) => {
                offsets.insert(id, pos);
                pos = after;
            }
            None => pos += 1,
        }
    }

    let max_id = offsets.keys().max().copied();
    // Reuse the original trailer dictionary (it carries /Root, /Info, ...).
    let sx = match rfind_bytes(data, b"startxref") {
        Some(s) => s,
        None => bail!("PDF xref table broken and no startxref found by scan"),
    };
    let tpos = match rfind_bytes(&data[..sx], b"trailer") {
        Some(t) => t,
        None => bail!("PDF xref table broken and no trailer found by scan"),
    };
    if max_id.is_none() {
        bail!("PDF xref table broken and no objects found by scan");
    }
    let size = max_id.unwrap() + 1;
    let mut s = tpos + 7;
    while s < sx && data[s].is_ascii_whitespace() {
        s += 1;
    }
    let mut e = sx;
    while e > s && data[e - 1].is_ascii_whitespace() {
        e -= 1;
    }
    let trailer_dict = fix_trailer_size(&data[s..e], size);

    // Append a fresh xref table + trailer as an incremental update.
    // The startxref offset must point exactly at the "xref" keyword (lopdf's
    // parser has no leading-whitespace tolerance).
    let mut out = data.to_vec();
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    let xref_offset = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", size).as_bytes());
    for id in 0..size {
        match offsets.get(&id) {
            Some(off) => out.extend_from_slice(format!("{:010} 0 n \n", off).as_bytes()),
            None => out.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    out.extend_from_slice(b"trailer ");
    out.extend_from_slice(&trailer_dict);
    out.extend_from_slice(format!("\nstartxref\n{}\n%%EOF", xref_offset).as_bytes());

    Document::load_mem(&out)
        .map_err(|e| anyhow::anyhow!("PDF xref recovery failed: {}", e))
}

/// Parse an "id gen obj" header at `pos` (which must start with a digit).
/// Returns (object_id, position just past the "obj" keyword).
fn parse_object_header(data: &[u8], pos: usize) -> Option<(u32, usize)> {
    let mut p = pos;
    let id_start = p;
    while p < data.len() && data[p].is_ascii_digit() {
        p += 1;
    }
    let id_end = p;
    if id_end - id_start == 0 || id_end - id_start > 9 {
        return None;
    }
    let mut ws = 0usize;
    while p < data.len() && (data[p] == b' ' || data[p] == b'\t') {
        ws += 1;
        p += 1;
    }
    if ws == 0 {
        return None;
    }
    let gen_start = p;
    while p < data.len() && data[p].is_ascii_digit() {
        p += 1;
    }
    if p == gen_start || p - gen_start > 5 {
        return None;
    }
    ws = 0;
    while p < data.len() && (data[p] == b' ' || data[p] == b'\t') {
        ws += 1;
        p += 1;
    }
    if ws == 0 || !data[p..].starts_with(b"obj") {
        return None;
    }
    let id: u32 = std::str::from_utf8(&data[id_start..id_end])
        .ok()?
        .parse()
        .ok()?;
    Some((id, p + 3))
}

/// Rewrite `/Size` in a trailer dictionary and drop any `/Prev` reference
/// (the rebuilt xref already covers the whole file).
fn fix_trailer_size(dict: &[u8], size: u32) -> Vec<u8> {
    let mut out = dict.to_vec();

    // /Size <number>
    if let Some(i) = find_key(&out, b"/Size") {
        let mut p = i + 5;
        while p < out.len() && out[p].is_ascii_whitespace() {
            p += 1;
        }
        let num_start = p;
        while p < out.len() && out[p].is_ascii_digit() {
            p += 1;
        }
        if p > num_start {
            let new_val = format!("/Size {}", size).into_bytes();
            out.splice(i..p, new_val);
        }
    }

    // /Prev <number> or /Prev N G R
    if let Some(i) = find_key(&out, b"/Prev") {
        let mut p = i + 5;
        while p < out.len() && out[p].is_ascii_whitespace() {
            p += 1;
        }
        let mut end = p;
        // "N G R" reference form
        if p < out.len() && out[p].is_ascii_digit() {
            while end < out.len() && (out[end].is_ascii_digit() || out[end] == b' ') {
                end += 1;
            }
            if end >= out.len() || out[end] != b'R' {
                // Plain number form: stop at first non-digit.
                while end < out.len() && out[end].is_ascii_digit() {
                    end += 1;
                }
            } else {
                end += 1; // consume 'R'
            }
        }
        if end > p || (end == p) {
            out.drain(i..end);
        }
    }

    out
}

/// Last occurrence of `needle` in `haystack` (byte-slice rfind).
fn rfind_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

/// Find a PDF dictionary key, ensuring it is not part of a longer name.
fn find_key(data: &[u8], key: &[u8]) -> Option<usize> {
    let mut pos = 0usize;
    while let Some(off) = data[pos..].windows(key.len()).position(|w| w == key) {
        let i = pos + off;
        let before_ok = i == 0 || !data[i - 1].is_ascii_alphanumeric();
        let after = i + key.len();
        let after_ok = after >= data.len() || !data[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return Some(i);
        }
        pos = i + 1;
    }
    None
}
