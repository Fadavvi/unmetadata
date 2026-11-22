mod cli;
mod error;
mod formats;
mod processor;
mod report;
mod server;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::Parser;
use rayon::prelude::*;

use cli::Cli;
use processor::{collect_paths, process_file, ProcessOptions};

fn main() {
    let cli = Cli::parse();

    if cli.serve {
        let rt = tokio::runtime::Runtime::new().expect("failed to create async runtime");
        if let Err(e) = rt.block_on(server::serve(cli.verbose)) {
            eprintln!("server error: {e}");
            std::process::exit(1);
        }
        return;
    }

    if let Some(n) = cli.jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .expect("failed to set thread pool size");
    }

    // Validate: -o requires exactly one existing regular-file input.
    // Check existence first so a missing path reports "no such file"
    // instead of the misleading arity error.
    if cli.output.is_some() {
        if cli.paths.len() != 1 {
            eprintln!("error: --output requires exactly one file argument");
            std::process::exit(2);
        }
        let path = &cli.paths[0];
        if !path.exists() {
            eprintln!("error: no such file: {}", path.display());
            std::process::exit(2);
        }
        if !path.is_file() {
            eprintln!(
                "error: --output requires a regular file, not a directory: {}",
                path.display()
            );
            std::process::exit(2);
        }
    }

    if let Some(dir) = &cli.output_dir {
        if !dir.exists() {
            if let Err(e) = std::fs::create_dir_all(dir) {
                eprintln!("error: cannot create output directory {}: {}", dir.display(), e);
                std::process::exit(2);
            }
        }
    }

    let files = collect_paths(&cli.paths, cli.recursive);
    if files.is_empty() {
        if !cli.quiet {
            eprintln!("warning: no files found");
        }
        std::process::exit(0);
    }

    // Resolve outputs up-front so basename collisions that `--output-dir`
    // can create (same filename in different subdirectories with -r) are
    // caught before any parallel writes: the first file wins, later ones are
    // skipped with a warning instead of clobbering each other on rename.
    let mut claimed: std::collections::HashMap<PathBuf, PathBuf> = std::collections::HashMap::new();
    let jobs: Vec<(PathBuf, Option<PathBuf>, Option<String>)> = files
        .iter()
        .map(|path| {
            let output = resolve_output(path, cli.output.as_deref(), cli.output_dir.as_deref());
            match output {
                Some(o) => match claimed.insert(o.clone(), path.clone()) {
                    Some(prev) => (
                        path.clone(),
                        None,
                        Some(format!(
                            "output name collision with {}: both map to {}",
                            prev.display(),
                            o.display()
                        )),
                    ),
                    None => (path.clone(), Some(o), None),
                },
                None => (path.clone(), None, None),
            }
        })
        .collect();

    let opts = ProcessOptions {
        dry_run: cli.dry_run,
        verbose: cli.verbose,
        quiet: cli.quiet,
    };

    let had_error = Arc::new(AtomicBool::new(false));

    jobs.par_iter().for_each(|(path, output, skip_reason)| {
        if let Some(reason) = skip_reason {
            if !opts.quiet {
                eprintln!("warning: skipping {} - {}", path.display(), reason);
            }
            return;
        }

        match process_file(path, output.as_deref(), &opts) {
            Ok(result) if result.skipped => {
                // Unsupported files are skipped with a warning by default
                // (see README); -q suppresses it.
                if !opts.quiet {
                    eprintln!(
                        "warning: skipping {} - {}",
                        path.display(),
                        result.skip_reason.as_deref().unwrap_or("unknown")
                    );
                }
            }
            Ok(result) => {
                if !opts.quiet {
                    let action = if opts.dry_run { "dry " } else { "ok  " };
                    // Build the whole per-file block as one string and print
                    // it in a single call so parallel workers don't interleave
                    // lines from different files.
                    let mut block = format!("{}  {}", action, path.display());
                    if result.report.is_empty() {
                        if opts.verbose {
                            block.push_str(" - nothing to strip");
                        }
                    } else if opts.verbose {
                        for item in &result.report.items {
                            block.push_str(&format!("\n      - {}", item.summary));
                            if !item.fields.is_empty() {
                                block.push_str(&format!(
                                    "\n          fields: {}",
                                    item.fields.join(", ")
                                ));
                            }
                        }
                    }
                    println!("{}", block);
                }
            }
            Err(e) => {
                // Error's Display already includes the file path.
                eprintln!("error: {}", e);
                had_error.store(true, Ordering::Relaxed);
            }
        }
    });

    if had_error.load(Ordering::Relaxed) {
        std::process::exit(1);
    }
}

fn resolve_output(
    input: &std::path::Path,
    output: Option<&std::path::Path>,
    output_dir: Option<&std::path::Path>,
) -> Option<PathBuf> {
    if let Some(o) = output {
        return Some(o.to_path_buf());
    }
    if let Some(dir) = output_dir {
        if let Some(fname) = input.file_name() {
            return Some(dir.join(fname));
        }
    }
    None
}
