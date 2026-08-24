use anyhow::{bail, Result};
use crate::report::StripReport;

// Image resource IDs to strip
const STRIP_IDS: &[u16] = &[
    0x0404, // IPTC-NAA
    0x040C, // Thumbnail (old)
    0x040F, // ICC profile
    0x0411, // ICC untagged flag
    0x0416, // Unicode alpha names
    0x0419, // Print scale
    0x0422, // XMP metadata
    0x0424, // Print info
    0x0425, // Print style
    0x0426, // Macintosh NSPrintInfo
    0x0427, // Windows DEVMODE
    0x0428, // Auto save file path
    0x0429, // Auto save format
    0x042A, // Path selection state
];

pub fn strip(data: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    if data.len() < 26 || &data[0..4] != b"8BPS" {
        bail!("not a PSD file");
    }

    let mut report = StripReport::default();
    let mut out = Vec::with_capacity(data.len());

    // Fixed header: 26 bytes
    out.extend_from_slice(&data[0..26]);
    let mut pos = 26usize;

    // Color Mode Data section
    if pos + 4 > data.len() {
        bail!("truncated PSD");
    }
    let color_data_len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
    let color_section_size = 4 + color_data_len;
    out.extend_from_slice(&data[pos..pos + color_section_size]);
    pos += color_section_size;

    // Image Resources section
    if pos + 4 > data.len() {
        bail!("truncated PSD image resources");
    }
    let ir_section_len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
    let ir_start = pos + 4;
    let ir_end = ir_start + ir_section_len;
    if ir_end > data.len() {
        bail!("PSD image resources out of bounds");
    }

    // Parse and filter image resources
    let mut new_ir: Vec<u8> = Vec::new();
    let mut ir_pos = ir_start;
    while ir_pos + 10 <= ir_end {
        if &data[ir_pos..ir_pos + 4] != b"8BIM" {
            break;
        }
        let resource_id = u16::from_be_bytes(data[ir_pos + 4..ir_pos + 6].try_into().unwrap());

        // Pascal string (length byte + data, padded to even)
        let name_len = data[ir_pos + 6] as usize;
        let name_field_len = if (name_len + 1) % 2 == 0 { name_len + 1 } else { name_len + 2 };
        let data_len_pos = ir_pos + 6 + name_field_len;
        if data_len_pos + 4 > ir_end {
            break;
        }
        let resource_data_len =
            u32::from_be_bytes(data[data_len_pos..data_len_pos + 4].try_into().unwrap()) as usize;
        let _resource_data_end = data_len_pos + 4 + resource_data_len;
        // Data is padded to even byte
        let padded_len = if resource_data_len % 2 == 0 { resource_data_len } else { resource_data_len + 1 };
        let block_end = data_len_pos + 4 + padded_len;
        if block_end > ir_end {
            break;
        }

        if STRIP_IDS.contains(&resource_id) {
            report.add(format!("PSD resource 0x{:04X}", resource_id));
        } else {
            new_ir.extend_from_slice(&data[ir_pos..block_end]);
        }

        ir_pos = block_end;
    }

    // Write filtered image resources section
    let new_ir_len = new_ir.len() as u32;
    out.extend_from_slice(&new_ir_len.to_be_bytes());
    out.extend_from_slice(&new_ir);

    // Rest of file (Layer+Mask info, Image Data) unchanged
    out.extend_from_slice(&data[ir_end..]);

    Ok((out, report))
}
