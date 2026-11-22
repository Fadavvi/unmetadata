use crate::error::Error;
use crate::report::StripReport;

pub mod bmp;
pub mod flac;
pub mod gif;
pub mod isobmff;
pub mod jpeg;
pub mod mkv;
pub mod mp3;
pub mod office;
pub mod ogg;
pub mod pdf;
pub mod png;
pub mod psd;
pub mod svg;
pub mod tiff;
pub mod webp;
pub mod zip;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Jpeg,
    Png,
    WebP,
    Gif,
    Tiff,
    /// PSD/PSB Photoshop
    Psd,
    /// BMP - no standard metadata fields, passes through unchanged
    Bmp,
    /// HEIC, HEIF, AVIF, MP4, M4A, M4V - ISO Base Media File Format
    Isobmff,
    Pdf,
    /// DOCX, XLSX, PPTX, ODT, ODS, ODP (all ZIP-based office formats)
    OfficeXml,
    Mp3,
    Flac,
    Ogg,
    Mkv,
    Svg,
    /// Generic ZIP (archive-level comment stripping)
    Zip,
}

/// Detect file format from magic bytes. Returns None for unknown/unsupported formats.
pub fn detect(data: &[u8]) -> Option<Format> {
    if data.len() < 4 {
        return None;
    }

    // JPEG: FF D8 FF
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Format::Jpeg);
    }

    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(Format::Png);
    }

    // RIFF/WebP
    if data.starts_with(b"RIFF") && data.len() >= 12 && &data[8..12] == b"WEBP" {
        return Some(Format::WebP);
    }

    // GIF87a or GIF89a
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some(Format::Gif);
    }

    // TIFF: little-endian II 2A 00 or big-endian MM 00 2A
    if (data.starts_with(&[0x49, 0x49, 0x2A, 0x00]))
        || (data.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]))
    {
        return Some(Format::Tiff);
    }

    // PSD: 38 42 50 53 ("8BPS")
    if data.starts_with(b"8BPS") {
        return Some(Format::Psd);
    }

    // BMP: "BM" - no standard metadata fields, detected so it passes through
    // instead of being reported as unsupported.
    if data.starts_with(b"BM") {
        return Some(Format::Bmp);
    }

    // ISO Base Media File Format (MP4/HEIC/AVIF/M4A):
    // 4-byte size + ftyp box type at offset 4
    if data.len() >= 8 && &data[4..8] == b"ftyp" {
        return Some(Format::Isobmff);
    }

    // PDF: %PDF-
    if data.starts_with(b"%PDF-") {
        return Some(Format::Pdf);
    }

    // ZIP (DOCX/XLSX/PPTX/ODT/ODS/ODP are ZIP files): PK 03 04
    if data.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        // Peek inside to distinguish Office XML / ODF from plain ZIP.
        return Some(detect_zip_subtype(data));
    }

    // MP3: ID3 tag (49 44 33) or sync word FF Fx
    if data.starts_with(b"ID3")
        || (data[0] == 0xFF && (data[1] & 0xE0) == 0xE0)
    {
        return Some(Format::Mp3);
    }

    // FLAC: fLaC
    if data.starts_with(b"fLaC") {
        return Some(Format::Flac);
    }

    // OGG: OggS
    if data.starts_with(b"OggS") {
        return Some(Format::Ogg);
    }

    // Matroska / WebM: EBML header 1A 45 DF A3
    if data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some(Format::Mkv);
    }

    // SVG: XML declaration or <svg
    if looks_like_svg(data) {
        return Some(Format::Svg);
    }

    None
}

fn detect_zip_subtype(data: &[u8]) -> Format {
    // Scan the local file headers looking for office content-type markers.
    // This is a heuristic: read the first few filenames in the ZIP.
    let mut pos = 0usize;
    let mut checks = 0;
    while pos + 30 < data.len() && checks < 16 {
        if &data[pos..pos + 4] != &[0x50, 0x4B, 0x03, 0x04] {
            break;
        }
        let fname_len = u16::from_le_bytes([data[pos + 26], data[pos + 27]]) as usize;
        let extra_len = u16::from_le_bytes([data[pos + 28], data[pos + 29]]) as usize;
        let fname_start = pos + 30;
        let fname_end = fname_start + fname_len;
        if fname_end > data.len() {
            break;
        }
        let fname = &data[fname_start..fname_end];
        // Office Open XML marker files
        if fname == b"[Content_Types].xml" || fname == b"word/document.xml"
            || fname == b"xl/workbook.xml" || fname == b"ppt/presentation.xml"
            || fname == b"META-INF/manifest.xml" || fname == b"mimetype"
        {
            return Format::OfficeXml;
        }

        // General-purpose bit flag 3 means the sizes in this local header are
        // deferred to a data descriptor (common for streamed entries), so the
        // size-based advance below would land inside the file data. Scan for
        // the next local file header instead.
        let flags = u16::from_le_bytes([data[pos + 6], data[pos + 7]]);
        if flags & 0x08 != 0 {
            let search_from = fname_end + extra_len;
            match data[search_from..]
                .windows(4)
                .position(|w| w == &[0x50, 0x4B, 0x03, 0x04])
            {
                Some(off) => pos = search_from + off,
                None => break,
            }
        } else {
            let compressed_size = u32::from_le_bytes([
                data[pos + 18], data[pos + 19], data[pos + 20], data[pos + 21],
            ]) as usize;
            pos = fname_end + extra_len + compressed_size;
        }
        checks += 1;
    }
    Format::Zip
}

fn looks_like_svg(data: &[u8]) -> bool {
    let prefix = &data[..data.len().min(512)];
    // Lossy: one non-UTF-8 byte in the header shouldn't disqualify the file.
    let s = String::from_utf8_lossy(prefix);
    let trimmed = s.trim_start();
    trimmed.starts_with("<?xml") && trimmed.contains("<svg")
        || trimmed.starts_with("<svg")
        || trimmed.starts_with("<!DOCTYPE svg")
}

/// Dispatch stripping to the appropriate format handler.
pub fn strip(
    format: Format,
    data: &[u8],
    path: &str,
) -> std::result::Result<(Vec<u8>, StripReport), Error> {
    match format {
        Format::Jpeg => jpeg::strip(data),
        Format::Png => png::strip(data),
        Format::WebP => webp::strip(data),
        Format::Gif => gif::strip(data),
        Format::Tiff => tiff::strip(data),
        Format::Psd => psd::strip(data),
        Format::Bmp => bmp::strip(data),
        Format::Isobmff => isobmff::strip(data),
        Format::Pdf => pdf::strip(data),
        Format::OfficeXml => office::strip(data),
        Format::Mp3 => mp3::strip(data),
        Format::Flac => flac::strip(data),
        Format::Ogg => ogg::strip(data),
        Format::Mkv => mkv::strip(data),
        Format::Svg => svg::strip(data),
        Format::Zip => zip_fmt::strip(data),
    }
    .map_err(|e| Error::format(path, e.to_string()))
}

// Re-export zip format handler under an alias to avoid name collision with the `zip` crate.
mod zip_fmt {
    pub use super::zip::strip;
}

/// Field names of a Vorbis comment payload (the bytes *after* the ID header:
/// vendor-string length, vendor string, entry count, then - per the Xiph
/// v-comment spec - each entry is a 4-byte little-endian length followed by
/// "NAME=value" bytes, not NUL-terminated).
/// Names are returned in order of first appearance, deduplicated. A truncated
/// or malformed block yields whatever could be parsed.
pub(crate) fn vorbis_comment_names(payload: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    if payload.len() < 4 {
        return names;
    }
    let vlen = u32::from_le_bytes(payload[0..4].try_into().unwrap()) as usize;
    let mut pos = 4 + vlen;
    if pos.saturating_add(4) > payload.len() {
        return names;
    }
    let count = u32::from_le_bytes(payload[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;
    for _ in 0..count {
        if pos.saturating_add(4) > payload.len() {
            break;
        }
        let elen = u32::from_le_bytes(payload[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if pos.saturating_add(elen) > payload.len() {
            break;
        }
        let entry = &payload[pos..pos + elen];
        if let Some(eq) = entry.iter().position(|&b| b == b'=') {
            let name = String::from_utf8_lossy(&entry[..eq]).into_owned();
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
        pos += elen;
    }
    names
}
