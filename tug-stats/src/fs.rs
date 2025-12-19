use color_eyre::eyre::{Context, Result};
use ignore::overrides::OverrideBuilder;
use ignore::{WalkBuilder, WalkState};
use log::debug;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub fn read_working_copy(root: &Path) -> Result<HashMap<String, String>> {
    let files = Arc::new(Mutex::new(HashMap::new()));
    // Use a Mutex<Option<color_eyre::Report>> to capture the first fatal error
    let error = Arc::new(Mutex::new(None));

    // Cleanly exclude internal directories using overrides
    let mut overrides = OverrideBuilder::new(root);
    overrides.add("!.jj/")?;
    overrides.add("!.git/")?;
    let override_filter = overrides
        .build()
        .context("Failed to build path overrides")?;

    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .overrides(override_filter)
        .build_parallel();

    walker.run(|| {
        let files = Arc::clone(&files);
        let error = Arc::clone(&error);
        let root = root.to_path_buf();

        Box::new(move |result| {
            // Stop if an error was already found
            if error.lock().unwrap().is_some() {
                return WalkState::Quit;
            }

            let entry = match result {
                Ok(e) => e,
                Err(err) => {
                    let mut err_guard = error.lock().unwrap();
                    if err_guard.is_none() {
                        *err_guard =
                            Some(color_eyre::eyre::eyre!(err).wrap_err("Directory walk error"));
                    }
                    return WalkState::Quit;
                }
            };

            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                let path = entry.path();

                if let Ok(metadata) = path.metadata() {
                    if metadata.len() > 1_000_000 {
                        return WalkState::Continue;
                    }
                }

                match fs::read_to_string(path) {
                    Ok(content) => {
                        if let Ok(rel_path) = path.strip_prefix(&root) {
                            let key = rel_path.to_string_lossy().to_string();
                            files.lock().unwrap().insert(key, content);
                        }
                    }
                    Err(e) => {
                        debug!("Skipping unreadable file {:?}: {}", path, e);
                    }
                }
            }
            WalkState::Continue
        })
    });

    // Extract and return any captured error
    if let Some(report) = Arc::try_unwrap(error).unwrap().into_inner().unwrap() {
        return Err(report);
    }

    // Unwrap the files map
    let final_map = Arc::try_unwrap(files)
        .map_err(|_| color_eyre::eyre::eyre!("Failed to unwrap Arc"))?
        .into_inner()
        .unwrap();

    Ok(final_map)
}
