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
        if let Err(e) = rt.block_on(server::serve()) {
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

    // Validate: -o requires exactly one file input
    if cli.output.is_some() {
        let non_dir: Vec<_> = cli.paths.iter().filter(|p| p.is_file()).collect();
        if non_dir.len() != 1 || cli.paths.len() != 1 {
            eprintln!("error: --output requires exactly one file argument");
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

    let opts = ProcessOptions {
        dry_run: cli.dry_run,
        verbose: cli.verbose,
        quiet: cli.quiet,
    };

    let had_error = Arc::new(AtomicBool::new(false));

    files.par_iter().for_each(|path| {
        let output = resolve_output(path, cli.output.as_deref(), cli.output_dir.as_deref());

        match process_file(path, output.as_deref(), &opts) {
            Ok(result) if result.skipped => {
                if opts.verbose {
                    println!(
                        "skip  {} — {}",
                        path.display(),
                        result.skip_reason.as_deref().unwrap_or("unknown")
                    );
                }
            }
            Ok(result) => {
                if !opts.quiet {
                    let action = if opts.dry_run { "dry " } else { "ok  " };
                    if result.report.is_empty() {
                        if opts.verbose {
                            println!("{}  {} — nothing to strip", action, path.display());
                        }
                    } else {
                        println!("{}  {}", action, path.display());
                        if opts.verbose {
                            for item in &result.report.items {
                                println!("      - {}", item);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("error {}: {}", path.display(), e);
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
