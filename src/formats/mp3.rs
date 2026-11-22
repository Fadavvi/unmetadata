use anyhow::Result;
use crate::report::StripReport;

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut report = StripReport::default();
    let mut start = 0usize;
    let mut end = data.len();

    // Strip ID3v2 at the beginning
    if data.len() >= 10 && &data[0..3] == b"ID3" {
        let flags = data[5];
        let size = synchsafe(data[6], data[7], data[8], data[9]);
        let footer = if flags & 0x10 != 0 { 10 } else { 0 };
        let total = 10 + size + footer;
        if total <= data.len() {
            report.add_fields(
                format!("MP3 ID3v2 tag ({} bytes)", total),
                id3v2_frame_ids(&data[..total - footer]),
            );
            start = total;
        }
    }

    // Strip ID3v1 at the end (128-byte tag starting with "TAG")
    if end.saturating_sub(start) >= 128 && &data[end - 128..end - 125] == b"TAG" {
        report.add_fields(
            "MP3 ID3v1 tag (128 bytes)".to_string(),
            id3v1_field_names(&data[end - 128..end]),
        );
        end -= 128;
    }

    Ok((data[start..end].to_vec(), report))
}

/// Frame IDs of the ID3v2 tag (e.g. "TIT2", "APIC"), in order of first
/// appearance, deduplicated. Supports v2.2/2.3/2.4 frame headers; stops at
/// the first malformed header so a corrupt tag can't run past its end.
fn id3v2_frame_ids(tag: &[u8]) -> Vec<String> {
    let mut ids = Vec::new();
    if tag.len() < 10 || &tag[0..3] != b"ID3" {
        return ids;
    }
    let version = tag[3]; // 2, 3 or 4
    let (id_len, size_len) = if version == 2 { (3usize, 3) } else { (4, 4) };

    let mut pos = 10usize;
    while pos + id_len + size_len + 2 <= tag.len() {
        // Frame IDs are A-Z / 0-9; anything else means padding or garbage.
        if !tag[pos..pos + id_len].iter().all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
            break;
        }
        let size: usize = match version {
            2 => ((tag[pos + 3] as usize) << 16) | (tag[pos + 4] as usize) << 8 | tag[pos + 5] as usize,
            4 => synchsafe(tag[pos + 3], tag[pos + 4], tag[pos + 5], tag[pos + 6]),
            _ => u32::from_be_bytes(
                tag[pos + 3..pos + 7].try_into().unwrap(),
            ) as usize, // v2.3: plain big-endian size
        };
        let id = String::from_utf8_lossy(&tag[pos..pos + id_len]).into_owned();
        if !ids.contains(&id) {
            ids.push(id);
        }
        pos += id_len + size_len + 2 /* flags */;
        if pos.saturating_add(size) > tag.len() {
            break; // frame data runs past the tag - stop rather than loop
        }
        pos += size;
    }
    ids
}

/// Names of the non-empty ID3v1 fields. Fixed layout: "TAG" + 30B title
/// (3..33) + 30B artist (33..63) + 30B album (63..93) + 4B year (93..97)
/// + comment (97..125 in v1.0, 97..127 in v1.1) + 1B genre index (127).
fn id3v1_field_names(tag: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let nonempty = |r: std::ops::Range<usize>| tag[r].iter().any(|&b| b != 0 && b != b' ');

    if nonempty(3..33) {
        names.push("title".into());
    }
    if nonempty(33..63) {
        names.push("artist".into());
    }
    if nonempty(63..93) {
        names.push("album".into());
    }
    if nonempty(93..97) {
        names.push("year".into());
    }
    // Old-style v1.0 tags end the comment with \0 at byte 125; v1.1 uses all
    // 30 bytes.
    let comment_end = if tag[125] == 0 && tag[126] != 0xFF { 125 } else { 127 };
    if nonempty(97..comment_end) {
        names.push("comment".into());
    }
    // The genre index (255 = none) sits at byte 127 in both v1.0 and v1.1.
    if tag[127] < 255 {
        names.push("genre".into());
    }
    names
}

fn synchsafe(a: u8, b: u8, c: u8, d: u8) -> usize {
    ((a as usize & 0x7F) << 21)
        | ((b as usize & 0x7F) << 14)
        | ((c as usize & 0x7F) << 7)
        | (d as usize & 0x7F)
}
