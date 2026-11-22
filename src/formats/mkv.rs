use anyhow::{bail, Result};
use crate::report::StripReport;

// EBML element IDs of interest
const ID_SEGMENT: u32 = 0x18538067;
const ID_INFO: u32 = 0x1549A966;
const ID_TAGS: u32 = 0x1254C367;
const ID_SEEKHEAD: u32 = 0x144DCB4A;
const ID_TITLE: u32 = 0x7BA9;
const ID_DATE_UTC: u32 = 0x4461;
const ID_MUXING_APP: u32 = 0x4D80;
const ID_WRITING_APP: u32 = 0x5741;
// Tag tree elements (Matroska spec)
const ID_SIMPLE_TAG: u32 = 0x67C8;
const ID_TAG_NAME: u32 = 0x738C;

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut report = StripReport::default();
    let mut out = Vec::with_capacity(data.len());

    let mut pos = 0usize;
    while pos < data.len() {
        let (id, id_len) = read_vint_id(data, pos)?;
        let (size, size_len) = read_vint_size(data, pos + id_len)?;
        let header_end = pos + id_len + size_len;
        // Unknown-size marker: the element runs to the end of its container.
        let elem_end = if size == usize::MAX {
            data.len()
        } else {
            let end = header_end.saturating_add(size);
            if end > data.len() && id != ID_SEGMENT {
                bail!("MKV element out of bounds");
            }
            end.min(data.len())
        };

        if id == ID_SEGMENT {
            // Recurse into the Segment body first, then emit a fresh header
            // so the declared size matches the (possibly shrunk) body.
            let mut body = Vec::with_capacity(elem_end - header_end);
            strip_segment(&data[header_end..elem_end], &mut body, &mut report)?;
            write_ebml_elem(&mut out, ID_SEGMENT, &body);
            pos = elem_end;
        } else {
            out.extend_from_slice(&data[pos..elem_end]);
            pos = elem_end;
        }
    }

    Ok((out, report))
}

fn strip_segment(src: &[u8], dst: &mut Vec<u8>, report: &mut StripReport) -> Result<()> {
    // First pass: element boundaries.
    let mut elems: Vec<(usize, u32, usize, usize)> = Vec::new(); // (start, id, header_end, elem_end)
    let mut pos = 0usize;
    while pos < src.len() {
        let (id, id_len) = read_vint_id(src, pos)?;
        let (size, size_len) = read_vint_size(src, pos + id_len)?;
        let header_end = pos + id_len + size_len;
        // Unknown-size marker: the element runs to the end of its container.
        let elem_end = if size == usize::MAX {
            src.len()
        } else {
            header_end.saturating_add(size).min(src.len())
        };
        elems.push((pos, id, header_end, elem_end));
        pos = elem_end;
    }

    // SeekHead is a lookup table of element offsets: it goes stale as soon as
    // anything in the segment changes, so drop it only when this segment is
    // actually modified (otherwise keeping it preserves fast seeking).
    let mut will_strip = false;
    for &(_, id, header_end, elem_end) in &elems {
        if id == ID_TAGS || (id == ID_INFO && info_has_strippable(&src[header_end..elem_end])) {
            will_strip = true;
            break;
        }
    }

    // Second pass: emit.
    for &(start, id, header_end, elem_end) in &elems {
        match id {
            ID_TAGS => {
                report.add_fields(
                    "MKV Tags element".to_string(),
                    mkv_tag_names(&src[header_end..elem_end]),
                );
                // Drop entirely
            }
            ID_INFO => {
                let mut new_info = Vec::new();
                strip_info_children(&src[header_end..elem_end], &mut new_info, report)?;
                write_ebml_elem(dst, id, &new_info);
            }
            ID_SEEKHEAD if will_strip => {
                report.add("MKV SeekHead element".to_string());
            }
            _ => {
                dst.extend_from_slice(&src[start..elem_end]);
            }
        }
    }
    Ok(())
}

/// True when an Info payload contains at least one strippable child.
fn info_has_strippable(info_payload: &[u8]) -> bool {
    let mut pos = 0usize;
    while pos + 2 <= info_payload.len() {
        let (id, id_len) = match read_vint_id(info_payload, pos) {
            Ok(v) => v,
            Err(_) => break,
        };
        let (size, size_len) = match read_vint_size(info_payload, pos + id_len) {
            Ok(v) => v,
            Err(_) => break,
        };
        if matches!(id, ID_TITLE | ID_DATE_UTC | ID_MUXING_APP | ID_WRITING_APP) {
            return true;
        }
        let header_end = pos + id_len + size_len;
        let elem_end = if size == usize::MAX {
            info_payload.len()
        } else {
            header_end.saturating_add(size).min(info_payload.len())
        };
        pos = elem_end.max(pos + 1);
    }
    false
}

fn strip_info_children(
    src: &[u8],
    dst: &mut Vec<u8>,
    report: &mut StripReport,
) -> Result<()> {
    let mut pos = 0usize;
    while pos < src.len() {
        let (id, id_len) = read_vint_id(src, pos)?;
        let (size, size_len) = read_vint_size(src, pos + id_len)?;
        let header_end = pos + id_len + size_len;
        // Unknown-size marker: the element runs to the end of its container.
        let elem_end = if size == usize::MAX {
            src.len()
        } else {
            header_end.saturating_add(size).min(src.len())
        };

        match id {
            ID_TITLE => { report.add("MKV Title"); }
            ID_DATE_UTC => { report.add("MKV DateUTC"); }
            ID_MUXING_APP => { report.add("MKV MuxingApp"); }
            ID_WRITING_APP => { report.add("MKV WritingApp"); }
            _ => {
                dst.extend_from_slice(&src[pos..elem_end]);
            }
        }
        pos = elem_end;
    }
    Ok(())
}

/// Names of the tags inside a dropped Tags element (the Name strings of its
/// SimpleTag entries, e.g. "Title", "ARTIST"), deduplicated in order.
fn mkv_tag_names(tags_payload: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    walk_tags(tags_payload, &mut names);
    names
}

/// Recursively walk the Tags tree (Tags → Tag → SimpleTag) collecting Name
/// children of every SimpleTag.
fn walk_tags(src: &[u8], names: &mut Vec<String>) {
    let mut pos = 0usize;
    while pos + 2 <= src.len() {
        let (id, id_len) = match read_vint_id(src, pos) {
            Ok(v) => v,
            Err(_) => break,
        };
        let (size, size_len) = match read_vint_size(src, pos + id_len) {
            Ok(v) => v,
            Err(_) => break,
        };
        let header_end = pos + id_len + size_len;
        if header_end > src.len() {
            break;
        }
        let elem_end = (header_end + size).min(src.len());

        if id == ID_SIMPLE_TAG {
            // Look for a Name child inside this SimpleTag.
            let mut p2 = 0usize;
            while p2 < size && header_end + p2 < src.len() {
                let (cid, cid_len) = match read_vint_id(&src[header_end..elem_end], p2) {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let (csize, csize_len) =
                    match read_vint_size(&src[header_end..elem_end], p2 + cid_len) {
                        Ok(v) => v,
                        Err(_) => break,
                    };
                if cid == ID_TAG_NAME {
                    let sstart = header_end + cid_len + csize_len;
                    let send = if csize == usize::MAX { elem_end } else { (sstart + csize).min(elem_end) };
                    if sstart <= send {
                        let name = String::from_utf8_lossy(&src[sstart..send]).into_owned();
                        if !name.is_empty() && !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
                p2 = p2.saturating_add(cid_len + csize_len);
                if csize == usize::MAX {
                    break; // unknown-size child: stop walking this tag
                }
                p2 = p2.saturating_add(csize);
            }
        } else {
            // Container element: recurse into its children.
            walk_tags(&src[header_end..elem_end], names);
        }

        pos = elem_end.max(pos + 1);
    }
}

/// Write an EBML element with a computed header.
fn write_ebml_elem(dst: &mut Vec<u8>, id: u32, payload: &[u8]) {
    dst.extend_from_slice(&encode_id(id));
    dst.extend_from_slice(&encode_size(payload.len()));
    dst.extend_from_slice(payload);
}

// --- EBML variable-length integer parsing ---

/// Read an EBML element ID (1-4 bytes, leading 1-bit encodes length).
fn read_vint_id(data: &[u8], pos: usize) -> Result<(u32, usize)> {
    if pos >= data.len() {
        bail!("EBML: pos out of bounds");
    }
    let first = data[pos];
    let len = if first & 0x80 != 0 { 1 }
    else if first & 0x40 != 0 { 2 }
    else if first & 0x20 != 0 { 3 }
    else if first & 0x10 != 0 { 4 }
    else { bail!("EBML: invalid ID width") };
    if pos + len > data.len() {
        bail!("EBML: truncated ID");
    }
    // For IDs, keep the width marker (don't mask it out)
    let mut val = 0u32;
    for &b in &data[pos..pos + len] {
        val = (val << 8) | b as u32;
    }
    Ok((val, len))
}

/// Read an EBML data size (1-8 bytes, leading 1-bit encodes length).
/// Returns `usize::MAX` for the unknown-size marker (all value bits zero),
/// which means the element extends to the end of its container.
fn read_vint_size(data: &[u8], pos: usize) -> Result<(usize, usize)> {
    if pos >= data.len() {
        bail!("EBML: size pos out of bounds");
    }
    let first = data[pos];
    let len = if first & 0x80 != 0 { 1 }
    else if first & 0x40 != 0 { 2 }
    else if first & 0x20 != 0 { 3 }
    else if first & 0x10 != 0 { 4 }
    else if first & 0x08 != 0 { 5 }
    else if first & 0x04 != 0 { 6 }
    else if first & 0x02 != 0 { 7 }
    else if first & 0x01 != 0 { 8 }
    else { bail!("EBML: invalid size width") };

    if pos + len > data.len() {
        bail!("EBML: truncated size");
    }
    let mask = 0x80u8 >> (len - 1);
    let mut val = (data[pos] & !mask) as usize;
    for &b in &data[pos + 1..pos + len] {
        val = (val << 8) | b as usize;
    }
    // All value bits zero: the unknown-size marker when the vint is wider
    // than one byte; a single-byte 0x80 is an explicit empty element.
    Ok((if val == 0 && len > 1 { usize::MAX } else { val }, len))
}

fn encode_id(id: u32) -> Vec<u8> {
    if id <= 0xFF { vec![id as u8] }
    else if id <= 0xFFFF { vec![(id >> 8) as u8, id as u8] }
    else if id <= 0xFFFFFF { vec![(id >> 16) as u8, (id >> 8) as u8, id as u8] }
    else { vec![(id >> 24) as u8, (id >> 16) as u8, (id >> 8) as u8, id as u8] }
}

fn encode_size(n: usize) -> Vec<u8> {
    if n < 0x7F { vec![n as u8 | 0x80] }
    else if n < 0x3FFF { vec![((n >> 8) as u8) | 0x40, n as u8] }
    else if n < 0x1FFFFF { vec![((n >> 16) as u8) | 0x20, (n >> 8) as u8, n as u8] }
    // An L-byte vint holds up to 2^(8L-1) - 1. The old code fell through to
    // the 4-byte arm for any larger size and truncated it; encode wider.
    else if n < 0x100_0000 {
        vec![
            ((n >> 24) as u8) | 0x10,
            (n >> 16) as u8,
            (n >> 8) as u8,
            n as u8,
        ]
    } else if n < 0x80_0000_00 {
        vec![
            ((n >> 32) as u8) | 0x08,
            (n >> 24) as u8,
            (n >> 16) as u8,
            (n >> 8) as u8,
            n as u8,
        ]
    } else if n < 0x40_0000_0000 {
        vec![
            ((n >> 40) as u8) | 0x04,
            (n >> 32) as u8,
            (n >> 24) as u8,
            (n >> 16) as u8,
            (n >> 8) as u8,
            n as u8,
        ]
    } else if n < 0x2_0000_0000_00 {
        vec![
            ((n >> 48) as u8) | 0x02,
            (n >> 40) as u8,
            (n >> 32) as u8,
            (n >> 24) as u8,
            (n >> 16) as u8,
            (n >> 8) as u8,
            n as u8,
        ]
    } else {
        vec![
            ((n >> 56) as u8) | 0x01,
            (n >> 48) as u8,
            (n >> 40) as u8,
            (n >> 32) as u8,
            (n >> 24) as u8,
            (n >> 16) as u8,
            (n >> 8) as u8,
            n as u8,
        ]
    }
}
