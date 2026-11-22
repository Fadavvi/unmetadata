use anyhow::{bail, Result};

use crate::report::StripReport;

// VP8X flags byte bit positions (first byte of the 10-byte VP8X chunk data).
// Per the WebP container spec / libwebp (WebPFeatureFlags):
//   bit 1 animation (0x02), bit 2 XMP (0x04), bit 3 EXIF (0x08),
//   bit 4 alpha (0x10), bit 5 ICC profile (0x20).
const FLAG_ICCP: u8 = 0x20; // bit 5
const FLAG_EXIF: u8 = 0x08; // bit 3
const FLAG_XMP: u8 = 0x04;  // bit 2

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    if data.len() < 12 {
        bail!("WebP file too short");
    }
    if &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        bail!("not a WebP file");
    }

    let mut report = StripReport::default();
    let mut out: Vec<u8> = Vec::with_capacity(data.len());

    // Write RIFF+size+WEBP header; size will be patched at the end.
    out.extend_from_slice(&data[0..12]);

    let mut pos = 12usize;
    // Offset within `out` of the VP8X flags byte, if the chunk was copied.
    let mut vp8x_flags_out: Option<usize> = None;
    let mut stripped_exif = false;
    let mut stripped_xmp = false;
    let mut stripped_iccp = false;

    while pos + 8 <= data.len() {
        let fourcc = &data[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        // RIFF chunks are padded to an even byte boundary.
        let padded = chunk_size + (chunk_size & 1);
        let chunk_end = pos + 8 + padded;

        match fourcc {
            b"EXIF" => {
                report.add("WebP EXIF chunk");
                stripped_exif = true;
            }
            b"XMP " => {
                report.add("WebP XMP chunk");
                stripped_xmp = true;
            }
            b"ICCP" => {
                report.add("WebP ICCP chunk");
                stripped_iccp = true;
            }
            b"VP8X" if chunk_size >= 4 => {
                // flags byte is at out[current_len + 8] (after fourcc + size)
                vp8x_flags_out = Some(out.len() + 8);
                let end = chunk_end.min(data.len());
                out.extend_from_slice(&data[pos..end]);
            }
            _ => {
                let end = chunk_end.min(data.len());
                out.extend_from_slice(&data[pos..end]);
            }
        }

        if chunk_end > data.len() {
            break;
        }
        pos = chunk_end;
    }

    // Clear VP8X feature flags for stripped chunks.
    if let Some(off) = vp8x_flags_out {
        if off < out.len() {
            if stripped_iccp {
                out[off] &= !FLAG_ICCP;
            }
            if stripped_exif {
                out[off] &= !FLAG_EXIF;
            }
            if stripped_xmp {
                out[off] &= !FLAG_XMP;
            }
        }
    }

    // Patch RIFF file size (total bytes minus the 8-byte RIFF+size header).
    let riff_size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&riff_size.to_le_bytes());

    Ok((out, report))
}
