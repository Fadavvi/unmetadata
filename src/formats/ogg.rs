use std::borrow::Cow;

use anyhow::{bail, Result};
use crate::report::StripReport;

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut report = StripReport::default();
    let pages = parse_pages(data)?;
    if pages.is_empty() {
        return Ok((data.to_vec(), report));
    }

    // Walk packets across page boundaries: a packet may span multiple pages
    // (continuation bit) and one page may hold several packets, so the
    // comment header is located at *packet* granularity rather than by
    // checking whether a page payload happens to start with it.
    let mut out: Vec<u8> = Vec::with_capacity(data.len());
    let mut replaced = false;

    // Bytes of an in-flight comment packet that started on an earlier page.
    // Non-comment packets flow through verbatim (no carry-over), so files
    // without a strippable comment are re-paginated byte-identically.
    let mut carried: Option<Vec<u8>> = None;

    for page in &pages {
        let payload = &data[page.payload_start..page.payload_end];
        let segs = &data[page.seg_table_start..page.seg_table_start + page.n_segs];
        let (frags, mut open_tail) = split_fragments(payload, segs);

        // Rebuild the page packet by packet. Lacing values define packet
        // boundaries, so each complete packet gets its own lacing run ending
        // in a value < 255; re-lacing the whole payload as one blob would
        // merge packets that share a page into a single giant packet.
        let mut new_payload: Vec<u8> = Vec::with_capacity(payload.len());
        let mut lacing: Vec<u8> = Vec::new();
        let mut header_type = page.header_type;
        let mut frag_idx = 0usize;

        // A comment packet carried over from the previous page is completed
        // by this page's first fragment (if any). Its earlier bytes were held
        // back, so emit the full packet here - possibly replaced with an
        // empty comment - and clear the continuation bit: in the output the
        // packet starts fresh on this page.
        if let Some(mut full) = carried.take() {
            header_type &= !0x01;
            match frags.get(0) {
                Some(&(s, e)) => {
                    full.extend_from_slice(&payload[s..e]);
                    let replacement = replace_comment(&mut replaced, &mut report, &full);
                    push_packet(&mut new_payload, &mut lacing, &replacement);
                    frag_idx = 1;
                }
                None => {
                    // This page holds only more of the in-flight packet: its
                    // whole payload is continuation bytes (the open tail at
                    // offset 0). Consume it into `carried` and emit this page
                    // empty.
                    if let Some(t) = open_tail.take() {
                        full.extend_from_slice(&payload[t..]);
                    }
                    carried = Some(full);
                }
            }
        }

        // Remaining fragments are complete packets on this page.
        for &(s, e) in &frags[frag_idx..] {
            let pkt = &payload[s..e];
            let replacement = replace_comment(&mut replaced, &mut report, pkt);
            push_packet(&mut new_payload, &mut lacing, &replacement);
        }

        // A new packet starts at the tail and continues on the next page.
        // Hold it back only if it looks like a comment header (it will be
        // replaced when it completes); otherwise pass its bytes through as an
        // open fragment - laced all-255, joined by the next page's
        // continuation bit.
        if let Some(t) = open_tail {
            let tail = &payload[t..];
            if !replaced && is_comment_packet(tail) {
                carried = Some(tail.to_vec());
            } else {
                push_open(&mut new_payload, &mut lacing, tail);
            }
        }

        write_page(
            &mut out,
            page.serial,
            page.sequence,
            page.granule,
            header_type,
            &new_payload,
            &lacing,
        );
    }

    // Truncated stream: a comment packet was still open at EOF. Emit its
    // bytes as a final standalone page so nothing is lost.
    if let Some(full) = carried {
        let last = pages.last().unwrap();
        let mut seq = last.sequence;
        for b in seq.iter_mut() {
            *b = b.wrapping_add(1);
        }
        let mut lacing = Vec::new();
        lace(&mut lacing, full.len(), true);
        write_page(
            &mut out,
            last.serial,
            seq,
            last.granule,
            0x00,
            &full,
            &lacing,
        );
    }

    Ok((out, report))
}

struct OggPage {
    payload_start: usize,
    payload_end: usize,
    seg_table_start: usize,
    n_segs: usize,
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
        let seg_table_start = pos + 27;
        let seg_table_end = seg_table_start + n_segs;
        if seg_table_end > data.len() {
            bail!("truncated OGG segment table");
        }
        let payload_len: usize = data[seg_table_start..seg_table_end]
            .iter()
            .map(|&x| x as usize)
            .sum();
        let payload_start = seg_table_end;
        let payload_end = payload_start + payload_len;
        if payload_end > data.len() {
            bail!("truncated OGG page payload");
        }
        pages.push(OggPage {
            payload_start,
            payload_end,
            seg_table_start,
            n_segs,
            header_type,
            granule,
            serial,
            sequence,
        });
        pos = payload_end;
    }
    Ok(pages)
}

/// Split a page payload into complete packet fragments.
/// Returns (fragments, open_tail): each fragment is a byte range [start, end)
/// within the payload that ends with a final (<255) lacing segment, and
/// open_tail is Some(offset) when the last fragment's segments are all 255,
/// i.e. its packet continues onto the next page.
fn split_fragments(payload: &[u8], segs: &[u8]) -> (Vec<(usize, usize)>, Option<usize>) {
    let mut offsets = Vec::with_capacity(segs.len() + 1);
    let mut off = 0usize;
    for &l in segs {
        offsets.push(off);
        off += l as usize;
    }
    offsets.push(off); // end of payload
    debug_assert_eq!(off, payload.len());

    let mut frags: Vec<(usize, usize)> = Vec::new();
    let mut open_tail: Option<usize> = None;
    let mut j = 0usize;
    while j < segs.len() {
        let start = offsets[j];
        let mut k = j;
        while k < segs.len() && segs[k] == 255 {
            k += 1;
        }
        if k < segs.len() {
            // Packet completes at segment k (its lacing value is < 255).
            frags.push((start, offsets[k + 1]));
            j = k + 1;
        } else {
            open_tail = Some(start);
            break;
        }
    }
    (frags, open_tail)
}

/// True if `pkt` is a Vorbis (\x03vorbis) or Opus (OpusTags) comment header.
fn is_comment_packet(pkt: &[u8]) -> bool {
    (pkt.len() >= 7 && pkt[0] == 0x03 && &pkt[1..7] == b"vorbis")
        || (pkt.len() >= 8 && &pkt[0..8] == b"OpusTags")
}

/// If `pkt` is the not-yet-replaced comment header, record it and return an
/// empty replacement; otherwise return the packet unchanged.
fn replace_comment<'a>(replaced: &mut bool, report: &mut StripReport, pkt: &'a [u8]) -> Cow<'a, [u8]> {
    if !*replaced && is_comment_packet(pkt) {
        *replaced = true;
        let opus = pkt.starts_with(b"OpusTags");
        // Comment fields follow the 8-byte "OpusTags" / 7-byte "\x03vorbis"
        // ID header.
        let names = super::vorbis_comment_names(&pkt[if opus { 8 } else { 7 }..]);
        if opus {
            report.add_fields("OGG Opus tags header".to_string(), names);
        } else {
            report.add_fields("OGG Vorbis comment header".to_string(), names);
        }
        Cow::Owned(empty_comment(opus))
    } else {
        Cow::Borrowed(pkt)
    }
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

/// Append one complete packet to a page's payload and emit its lacing values.
fn push_packet(payload: &mut Vec<u8>, lacing: &mut Vec<u8>, pkt: &[u8]) {
    lace(lacing, pkt.len(), true);
    payload.extend_from_slice(pkt);
}

/// Append an open fragment that continues onto the next page to a page's
/// payload and emit its all-255 lacing values (no final value).
fn push_open(payload: &mut Vec<u8>, lacing: &mut Vec<u8>, pkt: &[u8]) {
    lace(lacing, pkt.len(), false);
    payload.extend_from_slice(pkt);
}

/// Emit lacing values for `len` bytes of one packet. A complete packet ends
/// with its remainder (< 255); an open fragment consists only of full
/// 255-byte segments and has no final value.
fn lace(lacing: &mut Vec<u8>, len: usize, complete: bool) {
    let mut remaining = len;
    while remaining >= 255 {
        lacing.push(255);
        remaining -= 255;
    }
    if complete {
        lacing.push(remaining as u8);
    } else {
        debug_assert_eq!(remaining, 0);
    }
}

fn write_page(
    out: &mut Vec<u8>,
    serial: [u8; 4],
    sequence: [u8; 4],
    granule: [u8; 8],
    header_type: u8,
    payload: &[u8],
    lacing: &[u8],
) {
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
