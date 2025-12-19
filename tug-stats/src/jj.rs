use color_eyre::eyre::{bail, Context, Result};
use rayon::prelude::*;
use std::collections::HashMap;
use std::process::Command;

pub fn get_tree_contents(rev: &str, path: &str) -> Result<HashMap<String, String>> {
    // 1. Get the list of files for the revision
    let output = Command::new("jj")
        .args(["--no-pager", "file", "list", "-r", rev, path])
        .output()
        .context("Failed to run 'jj file list'")?;

    if !output.status.success() {
        bail!(
            "jj file list error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8(output.stdout)?;
    let paths: Vec<String> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();

    if paths.is_empty() {
        return Ok(HashMap::new());
    }

    // 2. Fetch contents in parallel.
    // We collect into a Result<Vec<Option<...>>> first to handle any process errors.
    let contents_vec: Vec<Option<(String, String)>> = paths
        .par_iter()
        .map(|p| {
            let out = Command::new("jj")
                .args(["--no-pager", "--color=never", "file", "show", p, "-r", rev])
                .output()
                .with_context(|| format!("Failed to fetch content for {}", p))?;

            if !out.status.success() {
                // If it fails (e.g. binary file), we skip it
                return Ok(None);
            }

            let content = String::from_utf8_lossy(&out.stdout).to_string();
            Ok(Some((p.clone(), content)))
        })
        .collect::<Result<Vec<_>>>()?; // The '?' here unwraps the Result

    // 3. Now that we have a plain Vec, we can flatten and collect into a HashMap
    let map: HashMap<String, String> = contents_vec.into_iter().flatten().collect();

    Ok(map)
}

pub fn get_commit_info(rev: &str) -> Result<crate::types::CommitInfo> {
    let template = r#"
    change_id.shortest(1) ++ "\0" ++ change_id.shortest(4) ++ "\0" ++ change_id ++ "\0" ++
    commit_id.shortest(1) ++ "\0" ++ commit_id.shortest(4) ++ "\0" ++ commit_id ++ "\0" ++
    description
    "#;
    let template = template.replace('\n', " ").trim().to_string();

    let output = Command::new("jj")
        .args([
            "--no-pager",
            "--color=never",
            "log",
            "-r",
            rev,
            "--no-graph",
            "-T",
            &template,
        ])
        .output()
        .context("Failed to fetch commit info")?;

    if !output.status.success() {
        bail!("jj log error: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8(output.stdout)?;
    let parts: Vec<&str> = stdout.split('\0').collect();

    if parts.len() < 7 {
        return Ok(crate::types::CommitInfo {
            description: "".to_string(),
            change_id_prefix: "".to_string(),
            change_id_short: "".to_string(),
            change_id_full: "".to_string(),
            commit_id_prefix: "".to_string(),
            commit_id_short: "".to_string(),
            commit_id_full: "".to_string(),
        });
    }

    Ok(crate::types::CommitInfo {
        change_id_prefix: parts[0].to_string(),
        change_id_short: parts[1].to_string(),
        change_id_full: parts[2].to_string(),
        commit_id_prefix: parts[3].to_string(),
        commit_id_short: parts[4].to_string(),
        commit_id_full: parts[5].to_string(),
        description: parts[6].trim().to_string(),
    })
}
