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
            report.add(format!("MP3 ID3v2 tag ({} bytes)", total));
            start = total;
        }
    }

    // Strip ID3v1 at the end (128-byte tag starting with "TAG")
    if end.saturating_sub(start) >= 128 && &data[end - 128..end - 125] == b"TAG" {
        report.add("MP3 ID3v1 tag (128 bytes)");
        end -= 128;
    }

    Ok((data[start..end].to_vec(), report))
}

fn synchsafe(a: u8, b: u8, c: u8, d: u8) -> usize {
    ((a as usize & 0x7F) << 21)
        | ((b as usize & 0x7F) << 14)
        | ((c as usize & 0x7F) << 7)
        | (d as usize & 0x7F)
}
