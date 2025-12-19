use crate::types::FileStatus;
use imara_diff::{Algorithm, Diff, InternedInput};
use log::warn;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

pub fn compute_diff(
    new_files: &HashMap<String, String>,
    old_files: &HashMap<String, String>,
    old_manifest: &HashSet<String>,
) -> Vec<FileStatus> {
    let new_paths: HashSet<_> = new_files.keys().cloned().collect();

    // 1. Classify Paths
    let added_paths: HashSet<_> = new_paths.difference(old_manifest).cloned().collect();
    let mut deleted_paths: HashSet<_> = old_manifest.difference(&new_paths).cloned().collect();
    let common_paths: Vec<_> = old_manifest.intersection(&new_paths).cloned().collect();

    // 2. Check Modifications (Parallelized)
    let mut results: Vec<FileStatus> = common_paths
        .par_iter()
        .filter_map(|path| {
            let old = old_files.get(path)?;
            let new = new_files.get(path)?;

            if old != new {
                if log::log_enabled!(log::Level::Warn) && old.trim_end() == new.trim_end() {
                    warn!("Whitespace/Newline only diff detected in {}", path);
                }
                let (added, removed) = calculate_diff_stats(old, new);
                Some(FileStatus::Modified(path.clone(), added, removed))
            } else {
                None
            }
        })
        .collect();

    // 3. Check Renames (Parallel Scoring -> Serial Resolution)
    // We compute the best match for *every* added file against *all* deleted files in parallel.
    struct MatchCandidate {
        added_path: String,
        deleted_path: String,
        score: f64,
    }

    let mut rename_candidates: Vec<MatchCandidate> = added_paths
        .par_iter()
        .filter_map(|added_path| {
            let target_content = &new_files[added_path];
            // Find the single best match for this added file among all deleted files
            find_best_match(target_content, &deleted_paths, old_files).map(|(best_old, score)| {
                MatchCandidate {
                    added_path: added_path.clone(),
                    deleted_path: best_old,
                    score,
                }
            })
        })
        .collect();

    // Sort by score descending to prioritize exact matches (simulating greedy best-match)
    rename_candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

    let mut final_added_paths = added_paths.clone();

    // Resolve renames
    for cand in rename_candidates {
        if final_added_paths.contains(&cand.added_path)
            && deleted_paths.contains(&cand.deleted_path)
        {
            let old_content = &old_files[&cand.deleted_path];
            let new_content = &new_files[&cand.added_path];
            let (add_count, rm_count) = calculate_diff_stats(old_content, new_content);

            results.push(FileStatus::Renamed {
                old: cand.deleted_path.clone(),
                new: cand.added_path.clone(),
                added: add_count,
                removed: rm_count,
            });

            // Claim these paths
            final_added_paths.remove(&cand.added_path);
            deleted_paths.remove(&cand.deleted_path);
        }
    }

    // 4. Check Copies (Parallelized)
    // Copies don't consume the source, so we can just run this purely in parallel
    // and collect the results.
    let available_sources: Vec<&String> = old_files.keys().collect();

    // We convert HashSet to Vec for par_iter.
    // Note: We only check files that weren't already marked as Renames.
    let copy_results: Vec<FileStatus> = final_added_paths
        .par_iter()
        .filter_map(|added_path| {
            let target_content = &new_files[added_path];

            // Re-implement find_best_match logic inline or adapted for &Vec<&String>
            // to avoid cloning the HashSet for every thread if we passed it in.
            // Using the helper with a slight tweak or iterating locally:

            let mut best_src = None;
            let mut best_score = 0.5; // Threshold

            for src in &available_sources {
                // Optimization: Don't compare with self if name matches (though logic allows it)
                if let Some(source_content) = old_files.get(*src) {
                    let score = calculate_similarity(source_content, target_content);
                    if score > best_score {
                        best_score = score;
                        best_src = Some((*src).clone());
                    }
                }
            }

            if let Some(src) = best_src {
                // Determine if it's a copy
                if src != *added_path {
                    let (add_count, rm_count) =
                        calculate_diff_stats(&old_files[&src], target_content);
                    return Some(FileStatus::Copied {
                        src,
                        dest: added_path.clone(),
                        added: add_count,
                        removed: rm_count,
                    });
                }
            }
            None
        })
        .collect();

    // Add copies to results and remove from final_added_paths
    for res in copy_results {
        if let FileStatus::Copied { dest, .. } = &res {
            final_added_paths.remove(dest);
        }
        results.push(res);
    }

    // 5. Finalize Added/Deleted (Parallelized calculation of line counts)
    let added_flushed: Vec<FileStatus> = final_added_paths
        .par_iter()
        .map(|p| {
            let count = new_files
                .get(p)
                .map(|s| s.lines().count() as u32)
                .unwrap_or(0);
            FileStatus::Added(p.clone(), count)
        })
        .collect();
    results.extend(added_flushed);

    let deleted_flushed: Vec<FileStatus> = deleted_paths
        .par_iter()
        .map(|p| {
            let count = old_files
                .get(p)
                .map(|s| s.lines().count() as u32)
                .unwrap_or(0);
            FileStatus::Deleted(p.clone(), count)
        })
        .collect();
    results.extend(deleted_flushed);

    // Sort alphanumerically by target path
    results.sort_by(|a, b| {
        let path_a = get_sort_key(a);
        let path_b = get_sort_key(b);
        path_a.cmp(path_b)
    });

    results
}

fn get_sort_key(status: &FileStatus) -> &String {
    match status {
        FileStatus::Added(p, _) => p,
        FileStatus::Deleted(p, _) => p,
        FileStatus::Modified(p, _, _) => p,
        FileStatus::Renamed { new, .. } => new,
        FileStatus::Copied { dest, .. } => dest,
    }
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
    // This helper checks a specific subset (candidates) against one target
    // We can't par_iter here easily because it's called from inside a par_iter,
    // but the outer loop provides enough parallelism.
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
    // Optimization: Quick length check. If lengths differ drastically, similarity is low.
    let len1 = s1.len();
    let len2 = s2.len();
    let max_len = std::cmp::max(len1, len2);
    let min_len = std::cmp::min(len1, len2);

    // If one file is less than 50% the size of the other, they can't be > 50% similar
    if (min_len as f64 / max_len as f64) < 0.5 {
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
