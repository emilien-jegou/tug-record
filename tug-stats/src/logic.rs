use crate::types::FileStatus;
use imara_diff::{Algorithm, Diff, InternedInput};
use log::warn;
use std::collections::{HashMap, HashSet};

pub fn compute_diff(
    new_files: &HashMap<String, String>,
    old_files: &HashMap<String, String>,
    old_manifest: &HashSet<String>,
) -> Vec<FileStatus> {
    let mut results = Vec::new();
    let new_paths: HashSet<_> = new_files.keys().cloned().collect();

    // 1. Classify Paths
    let mut added_paths: HashSet<_> = new_paths.difference(old_manifest).cloned().collect();
    let mut deleted_paths: HashSet<_> = old_manifest.difference(&new_paths).cloned().collect();
    let common_paths: Vec<_> = old_manifest.intersection(&new_paths).cloned().collect();

    // 2. Check Modifications
    for path in common_paths {
        // FIX: Added '&' to borrow the path string
        if let (Some(old), Some(new)) = (old_files.get(&path), new_files.get(&path)) {
            if old != new {
                if log::log_enabled!(log::Level::Warn) && old.trim_end() == new.trim_end() {
                    warn!("Whitespace/Newline only diff detected in {}", path);
                }
                let (added, removed) = calculate_diff_stats(old, new);
                results.push(FileStatus::Modified(path.clone(), added, removed));
            }
        }
    }

    // 3. Check Renames
    let mut detected_renames = HashSet::new();
    for added in &added_paths {
        if let Some((best_old, _)) = find_best_match(&new_files[added], &deleted_paths, old_files) {
            let (add_count, rm_count) =
                calculate_diff_stats(&old_files[&best_old], &new_files[added]);

            results.push(FileStatus::Renamed {
                old: best_old.clone(),
                new: added.clone(),
                added: add_count,
                removed: rm_count,
            });
            detected_renames.insert(added.clone());
            deleted_paths.remove(&best_old);
        }
    }
    for r in &detected_renames {
        added_paths.remove(r);
    }

    // 4. Check Copies
    let available_sources: HashSet<_> = old_files.keys().cloned().collect();
    let mut detected_copies = HashSet::new();

    for added in &added_paths {
        if let Some((src, _)) = find_best_match(&new_files[added], &available_sources, old_files) {
            if src != *added {
                let (add_count, rm_count) =
                    calculate_diff_stats(&old_files[&src], &new_files[added]);

                results.push(FileStatus::Copied {
                    src,
                    dest: added.clone(),
                    added: add_count,
                    removed: rm_count,
                });
                detected_copies.insert(added.clone());
            }
        }
    }
    for c in &detected_copies {
        added_paths.remove(c);
    }

    // 5. Finalize Added/Deleted
    for p in added_paths {
        let count = new_files
            .get(&p)
            .map(|s| s.lines().count() as u32)
            .unwrap_or(0);
        results.push(FileStatus::Added(p.clone(), count));
    }
    for p in deleted_paths {
        let count = old_files
            .get(&p)
            .map(|s| s.lines().count() as u32)
            .unwrap_or(0);
        results.push(FileStatus::Deleted(p.clone(), count));
    }

    // Sort alphanumerically by target path
    results.sort_by(|a, b| {
        let path_a = match a {
            FileStatus::Added(p, _) => p,
            FileStatus::Deleted(p, _) => p,
            FileStatus::Modified(p, _, _) => p,
            FileStatus::Renamed { new, .. } => new,
            FileStatus::Copied { dest, .. } => dest,
        };
        let path_b = match b {
            FileStatus::Added(p, _) => p,
            FileStatus::Deleted(p, _) => p,
            FileStatus::Modified(p, _, _) => p,
            FileStatus::Renamed { new, .. } => new,
            FileStatus::Copied { dest, .. } => dest,
        };
        path_a.cmp(path_b)
    });

    results
}

// --- Helpers ---

fn calculate_diff_stats(s1: &str, s2: &str) -> (u32, u32) {
    if s1 == s2 {
        return (0, 0);
    }
    let input = InternedInput::new(s1, s2);
    let diff = Diff::compute(Algorithm::Histogram, &input);

    let mut added = 0;
    let mut removed = 0;

    for hunk in diff.hunks() {
        removed += hunk.before.end - hunk.before.start;
        added += hunk.after.end - hunk.after.start;
    }
    (added, removed)
}

fn find_best_match(
    target_content: &str,
    candidates: &HashSet<String>,
    sources: &HashMap<String, String>,
) -> Option<(String, f64)> {
    let mut best: Option<(String, f64)> = None;
    for cand in candidates {
        if let Some(source_content) = sources.get(cand) {
            let score = calculate_similarity(source_content, target_content);
            if score > 0.5 {
                match best {
                    Some((_, s)) => {
                        if score > s {
                            best = Some((cand.clone(), score));
                        }
                    }
                    None => best = Some((cand.clone(), score)),
                }
            }
        }
    }
    best
}

fn calculate_similarity(s1: &str, s2: &str) -> f64 {
    if s1 == s2 {
        return 1.0;
    }
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }
    let input = InternedInput::new(s1, s2);
    let diff = Diff::compute(Algorithm::Histogram, &input);

    let mut changes = 0;
    for hunk in diff.hunks() {
        changes += (hunk.before.end - hunk.before.start) + (hunk.after.end - hunk.after.start);
    }
    let total = input.before.len() + input.after.len();
    if total == 0 {
        return 1.0;
    }
    (total as f64 - changes as f64) / total as f64
}
