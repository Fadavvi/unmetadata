use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "unmetadata",
    version,
    about = "Strip all metadata from images, documents, audio, video, and more",
    long_about = None,
)]
pub struct Cli {
    /// Files or directories to process.
    /// Directories are processed non-recursively by default.
    #[arg(required_unless_present = "serve", value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Write cleaned copy to PATH (single-file input only)
    #[arg(short, long, value_name = "PATH", conflicts_with = "output_dir")]
    pub output: Option<PathBuf>,

    /// Write cleaned copies into DIR (preserves filenames)
    #[arg(long, value_name = "DIR", conflicts_with = "output")]
    pub output_dir: Option<PathBuf>,

    /// Recurse into subdirectories
    #[arg(short, long)]
    pub recursive: bool,

    /// Show what would be stripped; no files modified
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Log each stripped item per file
    #[arg(short, long, conflicts_with = "quiet")]
    pub verbose: bool,

    /// Suppress all output except errors
    #[arg(short, long, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Parallel worker threads [default: CPU count]
    #[arg(short, long, value_name = "N")]
    pub jobs: Option<usize>,

    /// Start the web UI server on http://localhost:3301
    #[arg(long)]
    pub serve: bool,
}
