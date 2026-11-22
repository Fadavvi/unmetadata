use anyhow::Result;

use crate::report::StripReport;

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    if data.len() < 13 {
        return Ok((data.to_vec(), StripReport::default()));
    }
    if &data[0..6] != b"GIF87a" && &data[0..6] != b"GIF89a" {
        return Ok((data.to_vec(), StripReport::default()));
    }

    let mut out = Vec::with_capacity(data.len());
    let mut report = StripReport::default();

    // Header (6 bytes)
    out.extend_from_slice(&data[0..6]);
    let mut pos = 6usize;

    // Logical Screen Descriptor (7 bytes)
    if pos + 7 > data.len() {
        out.extend_from_slice(&data[pos..]);
        return Ok((out, report));
    }
    out.extend_from_slice(&data[pos..pos + 7]);
    let packed = data[pos + 4];
    let gct_flag = (packed >> 7) & 1;
    let gct_size = (packed & 0x07) as usize;
    pos += 7;

    // Global Color Table
    if gct_flag != 0 {
        let gct_bytes = 3 * (1 << (gct_size + 1));
        if pos + gct_bytes > data.len() {
            out.extend_from_slice(&data[pos..]);
            return Ok((out, report));
        }
        out.extend_from_slice(&data[pos..pos + gct_bytes]);
        pos += gct_bytes;
    }

    // Data blocks
    while pos < data.len() {
        match data[pos] {
            0x3B => {
                // Trailer
                out.push(0x3B);
                break;
            }
            0x2C => {
                // Image Descriptor: 1 introducer + 9 fields = 10 bytes
                if pos + 10 > data.len() {
                    out.extend_from_slice(&data[pos..]);
                    pos = data.len();
                    continue;
                }
                out.extend_from_slice(&data[pos..pos + 10]);
                let img_packed = data[pos + 9];
                let lct_flag = (img_packed >> 7) & 1;
                let lct_size = (img_packed & 0x07) as usize;
                pos += 10;

                if lct_flag != 0 {
                    let lct_bytes = 3 * (1 << (lct_size + 1));
                    if pos + lct_bytes > data.len() {
                        out.extend_from_slice(&data[pos..]);
                        pos = data.len();
                        continue;
                    }
                    out.extend_from_slice(&data[pos..pos + lct_bytes]);
                    pos += lct_bytes;
                }

                // LZW minimum code size byte + image data sub-blocks
                if pos < data.len() {
                    out.push(data[pos]);
                    pos += 1;
                    copy_sub_blocks(data, &mut pos, &mut out);
                }
            }
            0x21 => {
                // Extension block
                if pos + 1 >= data.len() {
                    out.push(data[pos]);
                    break;
                }
                let label = data[pos + 1];
                if label == 0xFE {
                    // Comment Extension - skip and report
                    pos += 2;
                    let mut comment_bytes = 0usize;
                    while pos < data.len() {
                        let len = data[pos] as usize;
                        pos += 1;
                        if len == 0 {
                            break;
                        }
                        comment_bytes += len;
                        pos += len.min(data.len() - pos);
                    }
                    report.add(format!("GIF comment extension ({comment_bytes} bytes)"));
                } else {
                    // All other extensions (Graphic Control, Application, Plain Text) are kept.
                    out.push(data[pos]);
                    out.push(data[pos + 1]);
                    pos += 2;
                    copy_sub_blocks(data, &mut pos, &mut out);
                }
            }
            b => {
                out.push(b);
                pos += 1;
            }
        }
    }

    Ok((out, report))
}

fn copy_sub_blocks(data: &[u8], pos: &mut usize, out: &mut Vec<u8>) {
    loop {
        if *pos >= data.len() {
            break;
        }
        let len = data[*pos] as usize;
        out.push(data[*pos]);
        *pos += 1;
        if len == 0 {
            break;
        }
        let end = (*pos + len).min(data.len());
        out.extend_from_slice(&data[*pos..end]);
        *pos = end;
    }
}
