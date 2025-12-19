mod fs;
mod histogram;
mod jj;
mod logic;
mod types;

use clap::Parser;
use color_eyre::eyre::{Context, Result};
use log::{debug, info};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::types::OutputFormat;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Path to the repository or directory to analyze
    #[arg(short, long, default_value = ".")]
    path: PathBuf,

    /// The revision to compare against (e.g., '@-' or a commit hash)
    #[arg(short, long, default_value = "@-")]
    revision: String,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    /// Show the full commit description in the header
    #[arg(long)]
    description: bool,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    color_eyre::install()?;

    let args = Args::parse();
    let root = std::fs::canonicalize(&args.path).context("Invalid path")?;
    std::env::set_current_dir(&root)?;

    info!("Streaming repository data...");

    let (to_info, from_info, old_files_content, new_files) = std::thread::scope(|s| {
        let t1 = s.spawn(|| jj::get_commit_info("@"));
        let t2 = s.spawn(|| jj::get_commit_info(&args.revision));
        let t3 = s.spawn(|| jj::get_tree_contents(&args.revision, "."));
        let t4 = s.spawn(|| fs::read_working_copy(&root));

        (
            t1.join().unwrap(),
            t2.join().unwrap(),
            t3.join().unwrap(),
            t4.join().unwrap(),
        )
    });

    let to_info = to_info?;
    let from_info = from_info?;
    let old_files_content = old_files_content?;
    let new_files = new_files?;

    let old_manifest: HashSet<String> = old_files_content.keys().cloned().collect();

    debug!("Calculating diffs...");
    let changes = logic::compute_diff(&new_files, &old_files_content, &old_manifest);

    match args.format {
        OutputFormat::Text => histogram::print_histogram(&to_info, &changes, args.description),
        OutputFormat::Json => types::print_json(&from_info, &to_info, &changes)?,
    }

    Ok(())
}
