use crate::types::CommitInfo;
use color_eyre::eyre::{bail, Context, Result};
use std::collections::HashSet;
use std::process::Command;

pub fn get_file_list(rev: &str) -> Result<HashSet<String>> {
    let output = Command::new("jj")
        .args(["--no-pager", "file", "list", "-r", rev])
        .output()
        .context("Failed to run 'jj file list'")?;

    if !output.status.success() {
        bail!("jj error: {}", String::from_utf8_lossy(&output.stderr));
    }
    let stdout = String::from_utf8(output.stdout).context("Invalid UTF-8 in jj output")?;
    Ok(stdout.lines().map(|s| s.to_string()).collect())
}

pub fn get_file_content(path: &str, rev: &str) -> Result<String> {
    let output = Command::new("jj")
        .args([
            "--no-pager",
            "--color=never",
            "file",
            "show",
            path,
            "-r",
            rev,
        ])
        .output()
        .context("Failed to execute jj file show")?;

    if !output.status.success() {
        bail!(
            "jj file show failed for '{}': {}",
            path,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn get_commit_info(rev: &str) -> Result<CommitInfo> {
    // Template uses \0 as separator to safely handle multiline descriptions
    let template = r#"
    change_id.shortest(1) ++ "\0" ++ change_id.shortest(4) ++ "\0" ++ change_id ++ "\0" ++
    commit_id.shortest(1) ++ "\0" ++ commit_id.shortest(4) ++ "\0" ++ commit_id ++ "\0" ++
    description
    "#;

    // Clean up template string for command line
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
    // Split by null byte
    let parts: Vec<&str> = stdout.split('\0').collect();

    if parts.len() < 7 {
        Ok(CommitInfo {
            description: "(no description available)".to_string(),
            change_id_prefix: "unknown".to_string(),
            change_id_short: "unknown".to_string(),
            change_id_full: "unknown".to_string(),
            commit_id_prefix: "unknown".to_string(),
            commit_id_short: "unknown".to_string(),
            commit_id_full: "unknown".to_string(),
        })
    } else {
        Ok(CommitInfo {
            change_id_prefix: parts[0].to_string(),
            change_id_short: parts[1].to_string(),
            change_id_full: parts[2].to_string(),
            commit_id_prefix: parts[3].to_string(),
            commit_id_short: parts[4].to_string(),
            commit_id_full: parts[5].to_string(),
            // Trim outer whitespace, but keep internal newlines
            description: parts[6].trim().to_string(),
        })
    }
}
