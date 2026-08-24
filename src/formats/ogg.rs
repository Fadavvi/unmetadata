use anyhow::{bail, Result};
use crate::report::StripReport;

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut report = StripReport::default();
    let pages = parse_pages(data)?;
    if pages.is_empty() {
        return Ok((data.to_vec(), report));
    }

    let mut out: Vec<u8> = Vec::with_capacity(data.len());
    let mut replaced = false;

    for page in &pages {
        let payload = &data[page.payload_start..page.payload_end];

        // Detect comment packet: Vorbis (\x03vorbis) or Opus (OpusTags)
        if !replaced && is_comment_packet(payload) {
            let packet_type = if payload.starts_with(b"OpusTags") {
                report.add("OGG Opus tags header");
                "Opus"
            } else {
                report.add("OGG Vorbis comment header");
                "Vorbis"
            };

            // Build minimal comment packet
            let empty = empty_comment(packet_type == "Opus");
            write_page(
                &mut out,
                page.serial,
                page.sequence,
                page.granule,
                page.header_type,
                &empty,
            );
            replaced = true;
        } else {
            out.extend_from_slice(&data[page.raw_start..page.raw_end]);
        }
    }

    Ok((out, report))
}

struct OggPage {
    raw_start: usize,
    raw_end: usize,
    payload_start: usize,
    payload_end: usize,
    header_type: u8,
    granule: [u8; 8],
    serial: [u8; 4],
    sequence: [u8; 4],
}

fn parse_pages(data: &[u8]) -> Result<Vec<OggPage>> {
    let mut pages = Vec::new();
    let mut pos = 0;
    while pos + 27 <= data.len() {
        if &data[pos..pos + 4] != b"OggS" {
            bail!("invalid OGG sync");
        }
        let header_type = data[pos + 5];
        let granule: [u8; 8] = data[pos + 6..pos + 14].try_into().unwrap();
        let serial: [u8; 4] = data[pos + 14..pos + 18].try_into().unwrap();
        let sequence: [u8; 4] = data[pos + 18..pos + 22].try_into().unwrap();
        let n_segs = data[pos + 26] as usize;
        let seg_table_end = pos + 27 + n_segs;
        if seg_table_end > data.len() {
            bail!("truncated OGG segment table");
        }
        let payload_len: usize = data[pos + 27..seg_table_end].iter().map(|&x| x as usize).sum();
        let payload_start = seg_table_end;
        let payload_end = payload_start + payload_len;
        if payload_end > data.len() {
            bail!("truncated OGG page payload");
        }
        pages.push(OggPage {
            raw_start: pos,
            raw_end: payload_end,
            payload_start,
            payload_end,
            header_type,
            granule,
            serial,
            sequence,
        });
        pos = payload_end;
    }
    Ok(pages)
}

fn is_comment_packet(payload: &[u8]) -> bool {
    (payload.len() >= 7 && payload[0] == 0x03 && &payload[1..7] == b"vorbis")
        || (payload.len() >= 8 && &payload[0..8] == b"OpusTags")
}

fn empty_comment(opus: bool) -> Vec<u8> {
    let mut pkt = Vec::new();
    if opus {
        pkt.extend_from_slice(b"OpusTags");
    } else {
        pkt.extend_from_slice(b"\x03vorbis");
    }
    // vendor string length = 0
    pkt.extend_from_slice(&0u32.to_le_bytes());
    // user comment list length = 0
    pkt.extend_from_slice(&0u32.to_le_bytes());
    if !opus {
        pkt.push(0x01); // framing bit
    }
    pkt
}

fn write_page(
    out: &mut Vec<u8>,
    serial: [u8; 4],
    sequence: [u8; 4],
    granule: [u8; 8],
    header_type: u8,
    payload: &[u8],
) {
    // Build lacing values for payload
    let mut lacing: Vec<u8> = Vec::new();
    let mut remaining = payload.len();
    while remaining >= 255 {
        lacing.push(255);
        remaining -= 255;
    }
    lacing.push(remaining as u8);

    let header_start = out.len();
    out.extend_from_slice(b"OggS");
    out.push(0); // version
    out.push(header_type);
    out.extend_from_slice(&granule);
    out.extend_from_slice(&serial);
    out.extend_from_slice(&sequence);
    out.extend_from_slice(&[0u8; 4]); // checksum placeholder
    out.push(lacing.len() as u8);
    out.extend_from_slice(&lacing);
    out.extend_from_slice(payload);

    // Compute CRC-32 and patch
    let crc = ogg_crc32(&out[header_start..]);
    let crc_pos = header_start + 22;
    out[crc_pos..crc_pos + 4].copy_from_slice(&crc.to_le_bytes());
}

fn ogg_crc32(data: &[u8]) -> u32 {
    // Ogg uses CRC-32/MPEG-2 (0x04C11DB7, initial=0, no reflection)
    const POLY: u32 = 0x04C11DB7;
    let mut crc: u32 = 0;
    for &byte in data {
        crc ^= (byte as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ POLY
            } else {
                crc << 1
            };
        }
    }
    crc
}
