use anyhow::Result;
use img_parts::jpeg::Jpeg;
use img_parts::Bytes;

use crate::report::StripReport;

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut jpeg = Jpeg::from_bytes(Bytes::copy_from_slice(data))?;
    let mut report = StripReport::default();

    jpeg.segments_mut().retain(|seg| match seg.marker() {
        0xE1 => {
            report.add("JPEG APP1 (EXIF/XMP)");
            false
        }
        0xE2 => {
            report.add("JPEG APP2 (ICC/XMP)");
            false
        }
        0xED => {
            report.add("JPEG APP13 (IPTC)");
            false
        }
        0xE0..=0xEF => {
            report.add("JPEG APPn");
            false
        }
        0xFE => {
            report.add("JPEG COM (comment)");
            false
        }
        _ => true,
    });

    let out = jpeg.encoder().bytes().to_vec();
    Ok((out, report))
}
