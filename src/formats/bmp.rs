use anyhow::Result;

use crate::report::StripReport;

/// BMP has no standard metadata slots (no EXIF/XMP/IPTC), so the file passes
/// through unchanged. It is detected only so it isn't reported as
/// "unsupported" (the README documents BMP pass-through).
pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    Ok((data.to_vec(), StripReport::default()))
}
