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

        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;

        if is_ooxml && DOCPROPS.contains(&name.as_str()) {
            match name.as_str() {
                "docProps/core.xml" => report.add_fields(
                    "Office core.xml (author, dates)".to_string(),
                    xml_property_names(&buf, false),
                ),
                "docProps/app.xml" => report.add_fields(
                    "Office app.xml (application info)".to_string(),
                    xml_property_names(&buf, false),
                ),
                "docProps/custom.xml" => report.add_fields(
                    "Office custom.xml".to_string(),
                    xml_property_names(&buf, true),
                ),
                _ => {}
            }
            continue;
        }

        let content: Vec<u8> = if is_ooxml && name == "[Content_Types].xml" {
            filter_content_types(&buf)?
        } else if is_ooxml && name == "_rels/.rels" {
            filter_rels(&buf)?
        } else if is_odf && name == "meta.xml" {
            report.add_fields(
                "ODF meta.xml (author, dates, stats)".to_string(),
                xml_property_names(&buf, false),
            );
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
              <office:document-meta \
               xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\">\
              <office:meta/></office:document-meta>"
                .to_vec()
        } else if is_odf && name == "settings.xml" {
            // App-specific configuration items (per the ODF spec this part
            // may also carry dc: properties) - replace with an empty set.
            report.add("ODF settings.xml (app settings)".to_string());
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
              <office:settings \
               xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\"/>"
                .to_vec()
        } else if is_odf && name == "styles.xml" {
            // Default styles; per the ODF spec this part may also carry
            // dc: properties. Replacing it can change appearance slightly.
            report.add("ODF styles.xml (default styles)".to_string());
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
              <office:styles \
               xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\"/>"
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

/// Property names found in an office metadata XML part.
///
/// `attr_mode = false`: collects the qualified element names of all elements
/// except the document root (core.xml / app.xml / ODF meta.xml store one
/// element per property). `attr_mode = true`: collects `name="..."` attribute
/// values (custom.xml stores properties as named vector items).
fn xml_property_names(xml: &[u8], attr_mode: bool) -> Vec<String> {
    let mut reader = Reader::from_reader(xml);
    let mut names = Vec::new();
    let mut depth = 0usize;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if attr_mode {
                    for a in e.attributes().filter_map(|a| a.ok()) {
                        if a.key.as_ref() == b"name" {
                            let v = String::from_utf8_lossy(a.value.as_ref()).into_owned();
                            if !v.is_empty() && !names.contains(&v) {
                                names.push(v);
                            }
                        }
                    }
                } else if depth > 0 {
                    let n = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    // Skip structural wrappers, keep the property elements.
                    if n != "office:meta" && !names.contains(&n) {
                        names.push(n);
                    }
                }
                depth += 1;
            }
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Eof) => break,
            _ => {}
        }
    }

    names
}

/// Remove elements named `tag` (self-closing or paired) for which `skip`
/// returns true; everything else passes through unchanged.
fn filter_xml_elements(
    xml: &[u8],
    tag: &str,
    skip: impl Fn(&quick_xml::events::BytesStart<'_>) -> bool,
) -> Result<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut out = Writer::new(Vec::new());
    let mut skipping = false;

    loop {
        match reader.read_event()? {
            Event::Eof => break,
            Event::Start(e) if !skipping && e.name().as_ref() == tag.as_bytes() => {
                if skip(&e) {
                    skipping = true; // swallow until the matching End
                } else {
                    out.write_event(Event::Start(e))?;
                }
            }
            Event::End(e) if skipping && e.name().as_ref() == tag.as_bytes() => {
                skipping = false;
            }
            other => {
                if !skipping {
                    out.write_event(other)?;
                }
            }
        }
    }

    Ok(out.into_inner())
}

fn filter_content_types(xml: &[u8]) -> Result<Vec<u8>> {
    filter_xml_elements(xml, "Override", |e| {
        e.attributes().filter_map(|a| a.ok()).any(|a| {
            a.key.as_ref() == b"PartName"
                && std::str::from_utf8(a.value.as_ref())
                    .map(|s| s.contains("docProps"))
                    .unwrap_or(false)
        })
    })
}

fn filter_rels(xml: &[u8]) -> Result<Vec<u8>> {
    filter_xml_elements(xml, "Relationship", |e| {
        e.attributes().filter_map(|a| a.ok()).any(|a| {
            a.key.as_ref() == b"Target"
                && std::str::from_utf8(a.value.as_ref())
                    .map(|s| s.starts_with("docProps/"))
                    .unwrap_or(false)
        })
    })
}
