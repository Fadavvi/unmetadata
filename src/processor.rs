use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::Error;
use crate::formats;
use crate::report::StripReport;

pub struct ProcessOptions {
    pub dry_run: bool,
    pub verbose: bool,
    pub quiet: bool,
}

pub struct ProcessResult {
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
                report: StripReport::default(),
                skipped: true,
                skip_reason: Some("unsupported format".into()),
            });
        }
    };

    // formats::strip already wraps handler errors with the path.
    let (cleaned, report) = formats::strip(fmt, &data, &path_str)?;

    if !opts.dry_run {
        let dest = output.unwrap_or(path);
        write_atomic(dest, &cleaned)
            .map_err(|e| Error::io(dest.display().to_string(), e))?;
    }

    Ok(ProcessResult {
        report,
        skipped: false,
        skip_reason: None,
    })
}

// Process-wide counter so parallel writes never share a temp filename.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Write data atomically: write to a unique sibling temp file, then rename.
/// The temp name includes pid + a per-process sequence number, so two files
/// with the same basename (e.g. `-r --output-dir` over nested directories)
/// can't collide on one shared `.tmp` path and clobber each other's rename.
fn write_atomic(dest: &Path, data: &[u8]) -> std::io::Result<()> {
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = dest.file_name().and_then(|f| f.to_str()).unwrap_or("file");
    // Strip the extension manually (Path::with_extension panics on a
    // dot-prefixed replacement and files without extensions have none).
    let base = match file_name.rfind('.') {
        Some(i) if i > 0 => &file_name[..i],
        _ => file_name,
    };
    let tmp = dest.with_file_name(format!(
        "{}.unmetadata.tmp.{}-{}",
        base,
        std::process::id(),
        n
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
