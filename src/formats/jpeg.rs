use anyhow::Result;
use img_parts::jpeg::Jpeg;
use img_parts::Bytes;

use crate::report::StripReport;

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut jpeg = Jpeg::from_bytes(Bytes::copy_from_slice(data))?;
    let mut report = StripReport::default();

    jpeg.segments_mut().retain(|seg| match seg.marker() {
        0xE1 => {
            let (label, fields) = describe_app1(seg.contents());
            report.add_fields(label, fields);
            false
        }
        0xE2 => {
            let (label, fields) = describe_app2(seg.contents());
            report.add_fields(label, fields);
            false
        }
        0xED => {
            report.add("JPEG APP13 (IPTC)".to_string());
            false
        }
        0xE0..=0xEF => {
            report.add(format!("JPEG APP{}", seg.marker() - 0xE0));
            false
        }
        0xFE => {
            report.add("JPEG COM (comment)".to_string());
            false
        }
        _ => true,
    });

    let out = jpeg.encoder().bytes().to_vec();
    Ok((out, report))
}

/// Label + field names for an APP1 payload (EXIF or XMP).
fn describe_app1(payload: &[u8]) -> (String, Vec<String>) {
    if payload.starts_with(b"Exif\0\0") {
        ("JPEG EXIF".to_string(), exif_tag_names(&payload[6..]))
    } else if is_xmp_packet(payload) {
        ("JPEG XMP (APP1)".to_string(), Vec::new())
    } else {
        ("JPEG APP1 (unknown payload)".to_string(), Vec::new())
    }
}

/// Label + field names for an APP2 payload (ICC profile or XMP).
fn describe_app2(payload: &[u8]) -> (String, Vec<String>) {
    if payload.starts_with(b"ICC_PROFILE\0") || payload.starts_with(b"ICCRGB") {
        ("JPEG ICC profile (APP2)".to_string(), Vec::new())
    } else if is_xmp_packet(payload) {
        ("JPEG XMP (APP2)".to_string(), Vec::new())
    } else {
        ("JPEG APP2 (unknown payload)".to_string(), Vec::new())
    }
}

/// Heuristic: does this segment payload look like an XMP packet?
fn is_xmp_packet(payload: &[u8]) -> bool {
    let head = std::str::from_utf8(&payload[..payload.len().min(200)]).unwrap_or("");
    head.contains("xpacket") || head.contains("xmpmeta") || head.contains("adobe.com/xap")
}

/// Tag names found in an EXIF TIFF structure (the bytes after the
/// "Exif\0\0" prefix): IFD0 plus the Exif and GPS sub-IFDs it points to.
fn exif_tag_names(tiff: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    if tiff.len() < 8 {
        return names;
    }
    let le = &tiff[0..2] == b"II";
    if !le && !tiff.starts_with(b"MM") || r16(tiff, 2, le) != 42 {
        return names;
    }

    // IFD0 plus the Exif (0x8769) and GPS (0x8825) sub-IFDs it points to.
    // Queue entries carry whether the IFD is the GPS one, whose tag numbers
    // get their own name table.
    let mut queue: Vec<(usize, bool)> = vec![(r32(tiff, 4, le) as usize, false)];
    let mut seen: Vec<usize> = Vec::new();
    while let Some((ifd, gps)) = queue.pop() {
        if seen.contains(&ifd) {
            continue;
        }
        seen.push(ifd);
        if ifd + 2 > tiff.len() {
            continue;
        }
        let count = r16(tiff, ifd, le) as usize;
        let entries_end = ifd + 2 + count * 12;
        if entries_end > tiff.len() {
            break;
        }
        for i in 0..count {
            let ep = ifd + 2 + i * 12;
            let tag = r16(tiff, ep, le);
            match tag {
                // Sub-IFD pointers: value is a TIFF-relative offset.
                0x8769 => queue.push((r32(tiff, ep + 8, le) as usize, false)),
                0x8825 => queue.push((r32(tiff, ep + 8, le) as usize, true)),
                _ => {
                    let name = if gps { gps_tag_name(tag) } else { exif_tag_name(tag) };
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
    }
    names
}

fn exif_tag_name(tag: u16) -> String {
    const NAMES: &[(u16, &str)] = &[
        (0x010E, "ImageDescription"),
        (0x010F, "Make"),
        (0x0110, "Model"),
        (0x0112, "Orientation"),
        (0x0131, "Software"),
        (0x0132, "DateTime"),
        (0x013B, "Artist"),
        (0x013C, "HostComputer"),
        (0x8298, "Copyright"),
        (0x9003, "DateTimeOriginal"),
        (0x9004, "DateTimeDigitized"),
    ];
    NAMES.iter()
        .find(|&&(t, _)| t == tag)
        .map(|&(_, n)| n.to_string())
        .unwrap_or_else(|| format!("tag 0x{:04X}", tag))
}

/// Tag names in the GPS IFD (its small tag numbers are distinct from the
/// IFD0/Exif ranges and need their own table).
fn gps_tag_name(tag: u16) -> String {
    const NAMES: &[(u16, &str)] = &[
        (0x0000, "GPSVersionID"),
        (0x0001, "GPSLatitudeRef"),
        (0x0002, "GPSLatitude"),
        (0x0003, "GPSLongitudeRef"),
        (0x0004, "GPSLongitude"),
        (0x0005, "GPSAltitudeRef"),
        (0x0006, "GPSAltitude"),
        (0x0007, "GPSDateStamp"),
        (0x0013, "GPSSatellites"),
        (0x0014, "GPSStatus"),
        (0x0015, "GPSMeasureMode"),
        (0x001D, "GPSProcessingMethod"),
    ];
    NAMES.iter()
        .find(|&&(t, _)| t == tag)
        .map(|&(_, n)| n.to_string())
        .unwrap_or_else(|| format!("tag 0x{:04X}", tag))
}

fn r16(data: &[u8], pos: usize, le: bool) -> u16 {
    let b: [u8; 2] = data[pos..pos + 2].try_into().unwrap();
    if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) }
}

fn r32(data: &[u8], pos: usize, le: bool) -> u32 {
    let b: [u8; 4] = data[pos..pos + 4].try_into().unwrap();
    if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) }
}
