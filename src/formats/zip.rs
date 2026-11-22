use anyhow::Result;
use crate::report::StripReport;
use std::io::{Cursor, Read, Write};

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut report = StripReport::default();
    let cursor = Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)?;

    let comment = archive.comment().to_vec();
    if !comment.is_empty() {
        report.add(format!("ZIP archive comment ({} bytes)", comment.len()));
    }

    let mut out_buf = Vec::with_capacity(data.len());
    {
        let out_cursor = Cursor::new(&mut out_buf);
        let mut writer = zip::ZipWriter::new(out_cursor);

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_owned();
            let compression = entry.compression();
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(compression);
            writer.start_file(name, options)?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            writer.write_all(&buf)?;
        }
        writer.finish()?;
    }

    Ok((out_buf, report))
}
