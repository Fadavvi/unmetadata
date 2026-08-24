use anyhow::{bail, Result};
use crate::report::StripReport;

// EBML element IDs of interest
const ID_SEGMENT: u32 = 0x18538067;
const ID_INFO: u32 = 0x1549A966;
const ID_TAGS: u32 = 0x1254C367;
const ID_TITLE: u32 = 0x7BA9;
const ID_DATE_UTC: u32 = 0x4461;
const ID_MUXING_APP: u32 = 0x4D80;
const ID_WRITING_APP: u32 = 0x5741;

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut report = StripReport::default();
    let mut out = Vec::with_capacity(data.len());

    let mut pos = 0usize;
    while pos < data.len() {
        let (id, id_len) = read_vint_id(data, pos)?;
        let (size, size_len) = read_vint_size(data, pos + id_len)?;
        let header_end = pos + id_len + size_len;
        let elem_end = header_end + size;
        if elem_end > data.len() && id != ID_SEGMENT {
            bail!("MKV element out of bounds");
        }
        let effective_end = elem_end.min(data.len());

        if id == ID_SEGMENT {
            // Write Segment header as-is, then recurse into its body
            out.extend_from_slice(&data[pos..header_end]);
            strip_segment(&data[header_end..effective_end], &mut out, &mut report)?;
            pos = effective_end;
        } else {
            out.extend_from_slice(&data[pos..effective_end]);
            pos = effective_end;
        }
    }

    Ok((out, report))
}

fn strip_segment(src: &[u8], dst: &mut Vec<u8>, report: &mut StripReport) -> Result<()> {
    let mut pos = 0usize;
    while pos < src.len() {
        let (id, id_len) = read_vint_id(src, pos)?;
        let (size, size_len) = read_vint_size(src, pos + id_len)?;
        let header_end = pos + id_len + size_len;
        let elem_end = (header_end + size).min(src.len());

        match id {
            ID_TAGS => {
                report.add("MKV Tags element");
                // Drop entirely
            }
            ID_INFO => {
                let mut new_info = Vec::new();
                strip_info_children(&src[header_end..elem_end], &mut new_info, report)?;
                write_ebml_elem(dst, id, &new_info);
            }
            _ => {
                dst.extend_from_slice(&src[pos..elem_end]);
            }
        }
        pos = elem_end;
    }
    Ok(())
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
        let elem_end = (header_end + size).min(src.len());

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
    Ok((val, len))
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
    else {
        vec![
            ((n >> 24) as u8) | 0x10,
            (n >> 16) as u8,
            (n >> 8) as u8,
            n as u8,
        ]
    }
}
