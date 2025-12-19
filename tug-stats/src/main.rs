mod fs;
mod histogram;
mod jj;
mod logic;
mod types;

use clap::Parser;
use color_eyre::eyre::{Context, Result};
use log::{debug, info, trace};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::types::OutputFormat;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(short, long, default_value = ".")]
    path: PathBuf,

    #[arg(short, long, default_value = "@-")]
    revision: String,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    #[arg(long)]
    description: bool,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    color_eyre::install()?;

    let args = Args::parse();
    let root = std::fs::canonicalize(&args.path).context("Invalid path")?;
    std::env::set_current_dir(&root)?;

    debug!("Root: {:?}, Rev: {}", root, args.revision);

    // 1. Parallel Initial Fetching
    debug!("Fetching initial data concurrently...");

    let (to_info, from_info, old_manifest, new_files) = std::thread::scope(|s| {
        let t1 = s.spawn(|| jj::get_commit_info("@"));
        let t2 = s.spawn(|| jj::get_commit_info(&args.revision));
        let t3 = s.spawn(|| jj::get_file_list(&args.revision));
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
    let old_manifest = old_manifest?;
    let new_files = new_files?;

    let new_manifest: HashSet<_> = new_files.keys().cloned().collect();
    trace!("old manifests: {old_manifest:?}");
    trace!("new manifests: {new_manifest:?}");

    // 2. Identify files to fetch
    let common_paths: Vec<_> = old_manifest.intersection(&new_manifest).collect();
    let deleted_paths: Vec<_> = old_manifest.difference(&new_manifest).collect();

    // Create a flat list of references to iterate over
    let paths_to_fetch: Vec<&String> = common_paths
        .iter()
        .chain(deleted_paths.iter())
        .cloned()
        .collect();

    // 3. Parallel Content Fetching
    debug!(
        "Fetching historical content for {} files...",
        paths_to_fetch.len()
    );

    let old_files_content: HashMap<String, String> = paths_to_fetch
        .par_iter()
        .map(|path| {
            let content = jj::get_file_content(path, &args.revision)
                .wrap_err_with(|| format!("Failed fetching {}", path))?;

            // FIX: path is &&String here. .to_string() creates the owned String we need.
            Ok((path.to_string(), content))
        })
        .collect::<Result<HashMap<_, _>>>()?;

    // 4. Compute Logic
    info!("Calculating diffs...");
    let changes = logic::compute_diff(&new_files, &old_files_content, &old_manifest);

    // 5. Print
    match args.format {
        OutputFormat::Text => histogram::print_histogram(&to_info, &changes, args.description),
        OutputFormat::Json => types::print_json(&from_info, &to_info, &changes)?,
    }

    Ok(())
}
