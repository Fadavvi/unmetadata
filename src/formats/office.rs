use anyhow::Result;
use std::io::{Cursor, Read, Write};
use zip::{ZipArchive, ZipWriter};
use zip::write::SimpleFileOptions;
use quick_xml::{Reader, Writer, events::Event};
use crate::report::StripReport;

const DOCPROPS: &[&str] = &[
    "docProps/core.xml",
    "docProps/app.xml",
    "docProps/custom.xml",
];

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut report = StripReport::default();

    let mut archive = ZipArchive::new(Cursor::new(data))?;

    let is_ooxml = archive.by_name("[Content_Types].xml").is_ok();
    let is_odf = !is_ooxml
        && (archive.by_name("META-INF/manifest.xml").is_ok()
            || archive.by_name("mimetype").is_ok());

    if !is_ooxml && !is_odf {
        return Ok((data.to_vec(), report));
    }

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        let options = SimpleFileOptions::default()
            .compression_method(entry.compression());

        if is_ooxml && DOCPROPS.contains(&name.as_str()) {
            match name.as_str() {
                "docProps/core.xml" => report.add("Office core.xml (author, dates)"),
                "docProps/app.xml" => report.add("Office app.xml (application info)"),
                "docProps/custom.xml" => report.add("Office custom.xml"),
                _ => {}
            }
            continue;
        }

        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;

        let content: Vec<u8> = if is_ooxml && name == "[Content_Types].xml" {
            filter_content_types(&buf)?
        } else if is_ooxml && name == "_rels/.rels" {
            filter_rels(&buf)?
        } else if is_odf && name == "meta.xml" {
            report.add("ODF meta.xml (author, dates, stats)");
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
              <office:document-meta \
               xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\">\
              <office:meta/></office:document-meta>"
                .to_vec()
        } else {
            buf
        };

        writer.start_file(name.as_str(), options)?;
        writer.write_all(&content)?;
    }

    let cursor = writer.finish()?;
    Ok((cursor.into_inner(), report))
}

fn filter_content_types(xml: &[u8]) -> Result<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut out = Writer::new(Vec::new());

    loop {
        match reader.read_event()? {
            Event::Eof => break,
            Event::Empty(e) if e.name().as_ref() == b"Override" => {
                let skip = e
                    .attributes()
                    .filter_map(|a| a.ok())
                    .any(|a| {
                        a.key.as_ref() == b"PartName"
                            && std::str::from_utf8(a.value.as_ref())
                                .map(|s| s.contains("docProps"))
                                .unwrap_or(false)
                    });
                if !skip {
                    out.write_event(Event::Empty(e))?;
                }
            }
            e => out.write_event(e)?,
        }
    }

    Ok(out.into_inner())
}

fn filter_rels(xml: &[u8]) -> Result<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut out = Writer::new(Vec::new());

    loop {
        match reader.read_event()? {
            Event::Eof => break,
            Event::Empty(e) if e.name().as_ref() == b"Relationship" => {
                let skip = e
                    .attributes()
                    .filter_map(|a| a.ok())
                    .any(|a| {
                        a.key.as_ref() == b"Target"
                            && std::str::from_utf8(a.value.as_ref())
                                .map(|s| s.starts_with("docProps/"))
                                .unwrap_or(false)
                    });
                if !skip {
                    out.write_event(Event::Empty(e))?;
                }
            }
            e => out.write_event(e)?,
        }
    }

    Ok(out.into_inner())
}
