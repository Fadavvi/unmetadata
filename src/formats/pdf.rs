use anyhow::Result;
use std::io::Cursor;
use lopdf::Document;
use crate::report::StripReport;

const INFO_FIELDS: &[(&[u8], &str)] = &[
    (b"Author", "Author"),
    (b"Creator", "Creator"),
    (b"Producer", "Producer"),
    (b"Subject", "Subject"),
    (b"Title", "Title"),
    (b"Keywords", "Keywords"),
    (b"CreationDate", "CreationDate"),
    (b"ModDate", "ModDate"),
    (b"Trapped", "Trapped"),
];

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let mut doc = Document::load_mem(data)?;
    let mut report = StripReport::default();

    // 1. Strip Info dictionary fields
    if let Ok(info_ref) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
        if let Ok(obj) = doc.get_object_mut(info_ref) {
            if let Ok(dict) = obj.as_dict_mut() {
                for (key, label) in INFO_FIELDS {
                    if dict.remove(*key).is_some() {
                        report.add(format!("PDF Info: {}", label));
                    }
                }
            }
        }
    }

    // 2. Find XMP metadata stream reference before mutably borrowing doc
    let root_id = doc.trailer.get(b"Root").and_then(|o| o.as_reference()).ok();

    let meta_id = root_id.and_then(|rid| {
        let obj = doc.get_object(rid).ok()?;
        let dict = obj.as_dict().ok()?;
        let meta = dict.get(b"Metadata").ok()?;
        meta.as_reference().ok()
    });

    // 3. Remove XMP stream and catalog reference
    if let Some(mid) = meta_id {
        if let Some(rid) = root_id {
            if let Ok(obj) = doc.get_object_mut(rid) {
                if let Ok(dict) = obj.as_dict_mut() {
                    dict.remove(b"Metadata");
                }
            }
        }
        doc.objects.remove(&mid);
        report.add("PDF XMP metadata stream");
    }

    let mut out = Cursor::new(Vec::new());
    doc.save_to(&mut out)?;
    Ok((out.into_inner(), report))
}
