use anyhow::{bail, Result};
use crate::report::StripReport;

// FLAC metadata block type numbers (https://flac.sourceforge.io/format.html):
// 0 STREAMINFO, 1 PADDING, 2 APPLICATION, 3 VORBIS_COMMENT, 4 PICTURE.
const VORBIS_COMMENT: u8 = 3;
const PICTURE: u8 = 4;

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    if data.len() < 4 || &data[0..4] != b"fLaC" {
        bail!("not a FLAC file");
    }

    let mut report = StripReport::default();
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(b"fLaC");

    let mut pos = 4usize;
    let mut pending: Vec<(u8, Vec<u8>)> = Vec::new(); // (type, payload) to write

    while pos + 4 <= data.len() {
        let hdr = data[pos];
        let is_last = hdr & 0x80 != 0;
        let block_type = hdr & 0x7F;
        let length = u24_be(&data[pos + 1..pos + 4]);
        let payload_start = pos + 4;
        let payload_end = payload_start + length;
        if payload_end > data.len() {
            bail!("FLAC block out of bounds");
        }
        let payload = &data[payload_start..payload_end];

        match block_type {
            VORBIS_COMMENT => {
                report.add_fields(
                    "FLAC VorbisComment block".to_string(),
                    super::vorbis_comment_names(payload),
                );
            }
            PICTURE => {
                report.add(format!("FLAC Picture block ({} bytes)", length));
            }
            t => {
                pending.push((t, payload.to_vec()));
            }
        }

        pos = payload_end;
        if is_last {
            break;
        }
    }

    // Write kept blocks; mark the last one with is_last bit
    let audio_data = &data[pos..];
    for (i, (block_type, payload)) in pending.iter().enumerate() {
        let is_last = i == pending.len() - 1;
        let hdr = if is_last { block_type | 0x80 } else { *block_type };
        let len = payload.len() as u32;
        out.push(hdr);
        out.extend_from_slice(&[(len >> 16) as u8, (len >> 8) as u8, len as u8]);
        out.extend_from_slice(payload);
    }

    // Append audio frames
    out.extend_from_slice(audio_data);

    Ok((out, report))
}

fn u24_be(b: &[u8]) -> usize {
    ((b[0] as usize) << 16) | ((b[1] as usize) << 8) | (b[2] as usize)
}
