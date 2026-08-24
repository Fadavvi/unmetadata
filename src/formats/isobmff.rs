use anyhow::{bail, Result};
use crate::report::StripReport;

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut report = StripReport::default();
    let mut out = Vec::with_capacity(data.len());
    strip_boxes(data, &mut out, &mut report, true)?;
    Ok((out, report))
}

/// Parse a sequence of boxes from `src`, writing kept boxes to `dst`.
/// `top_level` controls which box types trigger recursion vs stripping.
fn strip_boxes(
    src: &[u8],
    dst: &mut Vec<u8>,
    report: &mut StripReport,
    _top_level: bool,
) -> Result<()> {
    let mut pos = 0usize;
    while pos < src.len() {
        let (size, box_type, header_len) = parse_box(src, pos)?;
        let box_end = pos + size;
        if box_end > src.len() {
            bail!("ISOBMFF box extends past end of data");
        }
        let payload = &src[pos + header_len..box_end];

        match &box_type {
            b"meta" | b"udta" => {
                let name = std::str::from_utf8(&box_type).unwrap_or("??");
                report.add(format!("ISOBMFF {} box", name));
            }
            b"free" | b"skip" => {
                // Free space — drop silently.
            }
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"dinf" | b"edts" => {
                // Recurse: strip meta/udta inside these containers
                let mut inner = Vec::new();
                strip_boxes(payload, &mut inner, report, false)?;
                write_box(dst, &box_type, &inner);
            }
            _ => {
                dst.extend_from_slice(&src[pos..box_end]);
            }
        }

        pos = box_end;
    }
    Ok(())
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
