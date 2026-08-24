use anyhow::{bail, Result};
use crate::report::StripReport;

const META_TAGS: &[(u16, &str)] = &[
    (0x010E, "ImageDescription"),
    (0x010F, "Make"),
    (0x0110, "Model"),
    (0x0131, "Software"),
    (0x0132, "DateTime"),
    (0x013B, "Artist"),
    (0x013C, "HostComputer"),
    (0x8298, "Copyright"),
    (0x8769, "ExifIFD"),
    (0x8825, "GPSIFD"),
    (0x83BB, "IPTC"),
    (0x8649, "Photoshop"),
    (0x02BC, "XMP"),
    (0xC612, "DNGPrivate1"),
    (0xC613, "DNGPrivate2"),
    (0xC614, "DNGPrivate3"),
];

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    if data.len() < 8 {
        bail!("TIFF too short");
    }
    let le = match &data[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => bail!("invalid TIFF byte order marker"),
    };
    if r16(data, 2, le) != 42 {
        bail!("invalid TIFF magic");
    }

    let mut report = StripReport::default();
    // Work on a mutable copy; strip by removing IFD entries in-place.
    let mut out = data.to_vec();
    let first_ifd = r32(&out, 4, le) as usize;
    strip_ifd_chain(&mut out, first_ifd, le, &mut report);
    Ok((out, report))
}

/// Remove metadata entries from all IFDs in the chain starting at `ifd_offset`.
fn strip_ifd_chain(data: &mut Vec<u8>, ifd_offset: usize, le: bool, report: &mut StripReport) {
    let mut offset = ifd_offset;
    while offset != 0 && offset + 2 <= data.len() {
        let count = r16(data, offset, le) as usize;
        if offset + 2 + count * 12 + 4 > data.len() {
            break;
        }

        // Collect which entries to keep
        let mut kept = 0usize;
        for i in 0..count {
            let ep = offset + 2 + i * 12;
            let tag = r16(data, ep, le);
            if let Some(&(_, name)) = META_TAGS.iter().find(|&&(t, _)| t == tag) {
                report.add(format!("TIFF {}", name));
            } else {
                // Move entry to position `kept`
                if kept != i {
                    let src = offset + 2 + i * 12;
                    let dst = offset + 2 + kept * 12;
                    data.copy_within(src..src + 12, dst);
                }
                kept += 1;
            }
        }

        // Zero out the removed entries
        let zero_start = offset + 2 + kept * 12;
        let zero_end = offset + 2 + count * 12;
        for b in &mut data[zero_start..zero_end] {
            *b = 0;
        }

        // Update entry count
        w16(data, offset, kept as u16, le);

        // Follow next IFD pointer
        let next_ptr_pos = offset + 2 + count * 12;
        offset = r32(data, next_ptr_pos, le) as usize;
    }
}

fn r16(data: &[u8], pos: usize, le: bool) -> u16 {
    let b: [u8; 2] = data[pos..pos + 2].try_into().unwrap();
    if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) }
}

fn r32(data: &[u8], pos: usize, le: bool) -> u32 {
    let b: [u8; 4] = data[pos..pos + 4].try_into().unwrap();
    if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) }
}

fn w16(data: &mut [u8], pos: usize, val: u16, le: bool) {
    let b = if le { val.to_le_bytes() } else { val.to_be_bytes() };
    data[pos..pos + 2].copy_from_slice(&b);
}
