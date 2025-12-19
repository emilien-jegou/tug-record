use color_eyre::eyre::{Context, Result};
use ignore::WalkBuilder;
use log::debug;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub fn read_working_copy(root: &Path) -> Result<HashMap<String, String>> {
    let mut files = HashMap::new();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .build();

    for result in walker {
        let entry = result.context("Failed to walk directory entry")?;
        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            let path = entry.path();

            // Skip internal directories
            if path
                .components()
                .any(|c| c.as_os_str() == ".jj" || c.as_os_str() == ".git")
            {
                continue;
            }

            let rel_path = path
                .strip_prefix(root)
                .context("Failed to strip prefix path")?
                .to_string_lossy()
                .to_string();

            // Simple size guard (1MB)
            if let Ok(metadata) = path.metadata() {
                if metadata.len() > 1_000_000 {
                    debug!("Skipping large file: {}", rel_path);
                    continue;
                }
            }

            if let Ok(content) = fs::read_to_string(path) {
                files.insert(rel_path, content);
            }
        }
    }
    Ok(files)
}
