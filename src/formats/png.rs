use anyhow::Result;
use img_parts::png::Png;
use img_parts::Bytes;

use crate::report::StripReport;

const ITXT: [u8; 4] = *b"iTXt";
const TEXT: [u8; 4] = *b"tEXt";
const ZTXT: [u8; 4] = *b"zTXt";
const EXIF: [u8; 4] = *b"eXIf";
const TIME: [u8; 4] = *b"tIME";

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut png = Png::from_bytes(Bytes::copy_from_slice(data))?;
    let mut report = StripReport::default();

    png.chunks_mut().retain(|chunk| match chunk.kind() {
        ITXT => {
            report.add("PNG iTXt chunk");
            false
        }
        TEXT => {
            report.add("PNG tEXt chunk");
            false
        }
        ZTXT => {
            report.add("PNG zTXt chunk");
            false
        }
        EXIF => {
            report.add("PNG eXIf chunk");
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
