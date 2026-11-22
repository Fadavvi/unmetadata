use anyhow::{bail, Result};
use crate::report::StripReport;

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut report = StripReport::default();
    let mut out = strip_boxes(data, &mut report, true)?;

    // Boxes before mdat may have changed size (e.g. a dropped `free` box),
    // which shifts mdat and invalidates the absolute chunk offsets stored in
    // stco/co64 - patch them by the shift amount.
    patch_chunk_offsets(data, &mut out, &mut report)?;

    Ok((out, report))
}

/// Parse a sequence of boxes from `src`, writing kept boxes to `dst`.
/// At the top level, `meta` is essential structure for HEIF/AVIF images
/// (it holds item info/location/properties), so it is kept and recursed
/// into. Inside containers such as `moov`, `meta`/`udta` are metadata
/// containers (e.g. iTunes-style ilst) and are dropped wholesale.
fn strip_boxes(
    src: &[u8],
    report: &mut StripReport,
    top_level: bool,
) -> Result<Vec<u8>> {
    let mut dst = Vec::with_capacity(src.len());
    let mut pos = 0usize;
    while pos < src.len() {
        let (size, box_type, header_len) = parse_box(src, pos)?;
        let box_end = pos + size;
        if box_end > src.len() {
            bail!("ISOBMFF box extends past end of data");
        }
        let payload = &src[pos + header_len..box_end];

        match &box_type {
            b"meta" => {
                if top_level && !payload.is_empty() {
                    // HEIF/AVIF: `meta` is a full box - its first 4 bytes are
                    // version+flags, followed by child boxes (hdlr, iinf,
                    // iloc, iprp, ...). Keep the structure; strip nested
                    // metadata containers inside it.
                    if payload.len() < 4 {
                        bail!("truncated meta box");
                    }
                    let mut inner = Vec::with_capacity(payload.len());
                    inner.extend_from_slice(&payload[..4]);
                    strip_boxes(&payload[4..], report, false)?;
                    write_box(&mut dst, b"meta", &inner);
                } else {
                    // MP4/M4A: moov/meta holds iTunes-style metadata (ilst).
                    report.add_fields(
                        "ISOBMFF meta box".to_string(),
                        ilst_atom_names(payload),
                    );
                }
            }
            b"udta" => {
                report.add("ISOBMFF udta box");
            }
            b"free" | b"skip" => {
                // Free space - drop silently.
            }
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"dinf" | b"edts" => {
                // Recurse: strip meta/udta inside these containers
                let inner = strip_boxes(payload, report, false)?;
                write_box(&mut dst, &box_type, &inner);
            }
            b"mvhd" | b"tkhd" => {
                // Full boxes carrying creation/modification timestamps: zero
                // both (v0: u32 at payload offsets 4/8; v1: u64 at 4/12).
                let mut p = payload.to_vec();
                let time_end = match p.first().map(|b| b & 0x0F) {
                    Some(0) => 12,
                    Some(1) => 20,
                    _ => 0, // unknown version - leave untouched
                };
                if time_end > 0 && p.len() >= time_end {
                    for b in &mut p[4..time_end] {
                        *b = 0;
                    }
                    report.add(format!(
                        "ISOBMFF {} timestamps",
                        String::from_utf8_lossy(&box_type)
                    ));
                }
                write_box(&mut dst, &box_type, &p);
            }
            _ => {
                dst.extend_from_slice(&src[pos..box_end]);
            }
        }

        pos = box_end;
    }
    Ok(dst)
}

/// Parse the header of a box at `pos` in `data`.
/// Returns (total_size, type_bytes, header_len).
fn parse_box(data: &[u8], pos: usize) -> Result<(usize, [u8; 4], usize)> {
    if pos + 8 > data.len() {
        bail!("truncated box header");
    }
    let size_field = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
    let box_type: [u8; 4] = data[pos + 4..pos + 8].try_into().unwrap();

    let (total_size, header_len) = match size_field {
        0 => (data.len() - pos, 8),
        1 => {
            if pos + 16 > data.len() {
                bail!("truncated large box header");
            }
            let large =
                u64::from_be_bytes(data[pos + 8..pos + 16].try_into().unwrap()) as usize;
            (large, 16)
        }
        n => (n, 8),
    };

    Ok((total_size, box_type, header_len))
}

fn write_box(dst: &mut Vec<u8>, box_type: &[u8; 4], payload: &[u8]) {
    let size = (8 + payload.len()) as u32;
    dst.extend_from_slice(&size.to_be_bytes());
    dst.extend_from_slice(box_type);
    dst.extend_from_slice(payload);
}

/// Atom names inside a dropped `meta` box's `ilst` container (e.g.
/// "©nam (title)"), in order of first appearance. Returns an empty list when
/// the meta box has no ilst or it can't be parsed.
fn ilst_atom_names(meta_payload: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    // `meta` is a full box: 4 bytes of version/flags, then child boxes.
    if meta_payload.len() < 8 {
        return names;
    }
    let body = &meta_payload[4..];
    let mut pos = 0usize;
    while pos + 8 <= body.len() {
        let (size, box_type, header_len) = match parse_box(body, pos) {
            Ok(v) => v,
            Err(_) => break,
        };
        if size < header_len || pos + size > body.len() {
            break;
        }
        if &box_type == b"ilst" {
            let ilst = &body[pos + header_len..pos + size];
            let mut ip = 0usize;
            while ip + 8 <= ilst.len() {
                match parse_box(ilst, ip) {
                    Ok((atom_size, atom_type, atom_header)) => {
                        if atom_size < atom_header || ip + atom_size > ilst.len() {
                            break;
                        }
                        let label = atom_label(&atom_type);
                        if !names.contains(&label) {
                            names.push(label);
                        }
                        ip += atom_size;
                    }
                    Err(_) => break,
                }
            }
        }
        pos += size;
    }
    names
}

/// Human-readable label for a common iTunes metadata atom; the raw 4cc for
/// anything else.
fn atom_label(t: &[u8; 4]) -> String {
    let friendly = match t {
        b"\xA9nam" => Some("title"),
        b"aART" | b"\xA9art" => Some("artist"),
        b"\xA9alb" => Some("album"),
        b"trkn" => Some("track number"),
        b"gnre" => Some("genre"),
        b"covr" => Some("cover art"),
        b"\xA9day" => Some("year"),
        b"\xA9wrt" => Some("composer"),
        b"\xA9cmt" => Some("comment"),
        b"\xA9lyr" => Some("lyrics"),
        b"\xA9grp" => Some("grouping"),
        _ => None,
    };
    let raw = String::from_utf8_lossy(t).into_owned();
    match friendly {
        Some(f) => format!("{} ({})", raw, f),
        None => raw,
    }
}

/// File offset of the first top-level `mdat` box, if any.
fn find_mdat_offset(data: &[u8]) -> Result<Option<usize>> {
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let (size, box_type, _header_len) = parse_box(data, pos)?;
        if &box_type == b"mdat" {
            return Ok(Some(pos));
        }
        let end = (pos + size).min(data.len());
        if end <= pos {
            break;
        }
        pos = end;
    }
    Ok(None)
}

/// Add the mdat shift to every entry of the stco/co64 tables in `out`.
fn patch_chunk_offsets(orig: &[u8], out: &mut Vec<u8>, report: &mut StripReport) -> Result<()> {
    match (find_mdat_offset(orig)?, find_mdat_offset(out)?) {
        (Some(o), Some(n)) if o != n => {
            let delta = n as i64 - o as i64;
            if patch_stco_tables(out, delta)? {
                report.add(format!("ISOBMFF chunk offsets shifted by {delta:+}"));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Walk the container hierarchy of `data` and patch stco/co64 entries in place.
fn patch_stco_tables(data: &mut [u8], delta: i64) -> Result<bool> {
    let mut patched = false;
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let (size, box_type, header_len) = parse_box(data, pos)?;
        let end = (pos + size).min(data.len());
        match &box_type {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" => {
                if patch_stco_tables(&mut data[pos + header_len..end], delta)? {
                    patched = true;
                }
            }
            b"stco" | b"co64" => {
                let wide = &box_type == b"co64";
                if patch_one_offset_table(&mut data[pos + header_len..end], wide, delta)? {
                    patched = true;
                }
            }
            _ => {}
        }
        if end <= pos {
            break;
        }
        pos = end;
    }
    Ok(patched)
}

/// Add `delta` to each absolute offset in one stco (u32 entries) or co64
/// (u64 entries) table. The full-box layout is version/flags, entry count,
/// then the offsets.
fn patch_one_offset_table(payload: &mut [u8], wide: bool, delta: i64) -> Result<bool> {
    if payload.len() < 8 {
        return Ok(false);
    }
    let count = u32::from_be_bytes(payload[4..8].try_into().unwrap()) as usize;
    let width = if wide { 8 } else { 4 };
    for i in 0..count {
        let off = 8 + i * width;
        if off.saturating_add(width) > payload.len() {
            bail!("chunk offset table extends past end of box");
        }
        if wide {
            let v = u64::from_be_bytes(payload[off..off + 8].try_into().unwrap()) as i64;
            let nv = v.checked_add(delta).ok_or_else(|| anyhow::anyhow!("chunk offset overflow"))?;
            payload[off..off + 8].copy_from_slice(&(nv as u64).to_be_bytes());
        } else {
            let v = u32::from_be_bytes(payload[off..off + 4].try_into().unwrap()) as i64;
            let nv = v.checked_add(delta).ok_or_else(|| anyhow::anyhow!("chunk offset overflow"))?;
            payload[off..off + 4].copy_from_slice(&(nv as u32).to_be_bytes());
        }
    }
    Ok(count > 0)
}
