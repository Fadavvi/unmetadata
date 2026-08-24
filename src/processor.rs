use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::formats;
use crate::report::StripReport;

pub struct ProcessOptions {
    pub dry_run: bool,
    pub verbose: bool,
    pub quiet: bool,
}

pub struct ProcessResult {
    pub path: PathBuf,
    pub report: StripReport,
    pub skipped: bool,
    pub skip_reason: Option<String>,
}

/// Read, detect, strip, and write one file. Returns a ProcessResult.
pub fn process_file(
    path: &Path,
    output: Option<&Path>,
    opts: &ProcessOptions,
) -> Result<ProcessResult, Error> {
    let data = fs::read(path).map_err(|e| Error::io(path.display().to_string(), e))?;

    let path_str = path.display().to_string();

    let fmt = match formats::detect(&data) {
        Some(f) => f,
        None => {
            return Ok(ProcessResult {
                path: path.to_path_buf(),
                report: StripReport::default(),
                skipped: true,
                skip_reason: Some("unsupported format".into()),
            });
        }
    };

    let (cleaned, report) = formats::strip(fmt, &data, &path_str)
        .map_err(|e| Error::format(&path_str, e.to_string()))?;

    if !opts.dry_run {
        let dest = output.unwrap_or(path);
        write_atomic(dest, &cleaned)
            .map_err(|e| Error::io(dest.display().to_string(), e))?;
    }

    Ok(ProcessResult {
        path: path.to_path_buf(),
        report,
        skipped: false,
        skip_reason: None,
    })
}

/// Write data atomically: write to a sibling temp file, then rename.
fn write_atomic(dest: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = dest.with_extension(format!(
        "{}.tmp",
        dest.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    fs::write(&tmp, data)?;
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e
    })
}

/// Collect all file paths from the given input paths.
/// Directories are walked one level deep unless `recursive` is true.
pub fn collect_paths(inputs: &[PathBuf], recursive: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for input in inputs {
        if input.is_file() {
            out.push(input.clone());
        } else if input.is_dir() {
            let depth = if recursive { usize::MAX } else { 1 };
            let walker = walkdir::WalkDir::new(input)
                .min_depth(1)
                .max_depth(depth)
                .follow_links(false);
            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    out.push(entry.into_path());
                }
            }
        }
    }
    out
}
