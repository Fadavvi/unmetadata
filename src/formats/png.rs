use anyhow::Result;
use img_parts::png::Png;
use img_parts::Bytes;

use crate::report::StripReport;

const ITXT: [u8; 4] = *b"iTXt";
const TEXT: [u8; 4] = *b"tEXt";
const ZTXT: [u8; 4] = *b"zTXt";
const EXIF: [u8; 4] = *b"eXIf";
const TIME: [u8; 4] = *b"tIME";
const ICCP: [u8; 4] = *b"iCCP";

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut png = Png::from_bytes(Bytes::copy_from_slice(data))?;
    let mut report = StripReport::default();

    png.chunks_mut().retain(|chunk| match chunk.kind() {
        ITXT => {
            report.add_fields(
                "PNG iTXt chunk".to_string(),
                text_keywords(chunk.contents()),
            );
            false
        }
        TEXT => {
            report.add_fields(
                "PNG tEXt chunk".to_string(),
                text_keywords(chunk.contents()),
            );
            false
        }
        ZTXT => {
            report.add_fields(
                "PNG zTXt chunk".to_string(),
                text_keywords(chunk.contents()),
            );
            false
        }
        EXIF => {
            report.add("PNG eXIf chunk");
            false
        }
        ICCP => {
            report.add("PNG iCCP chunk (ICC profile)");
            false
        }
        TIME => {
            report.add("PNG tIME chunk");
            false
        }
        _ => true,
    });

    let out = png.encoder().bytes().to_vec();
    Ok((out, report))
}

/// Keyword(s) of a PNG text chunk. All three text chunk types (tEXt, iTXt,
/// zTXt) begin with the keyword followed by a NUL byte.
fn text_keywords(data: &[u8]) -> Vec<String> {
    match data.iter().position(|&b| b == 0) {
        Some(nul) => match std::str::from_utf8(&data[..nul]) {
            Ok(kw) if !kw.is_empty() => vec![kw.to_string()],
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}
