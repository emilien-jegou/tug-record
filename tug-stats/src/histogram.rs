use crate::types::{CommitInfo, FileStatus};
use colored::Colorize;
use std::cmp;
use terminal_size::{terminal_size, Width};

const MIN_GRAPH_WIDTH: usize = 10;

pub fn print_histogram(header: &CommitInfo, changes: &[FileStatus], show_full_desc: bool) {
    let (total_added, total_removed, max_change_count) =
        changes
            .iter()
            .fold((0, 0, 0), |(acc_a, acc_r, max_c), status| {
                let (a, r) = extract_stats(status);
                let total = a + r;
                (acc_a + a, acc_r + r, cmp::max(max_c, total))
            });

    let file_count = changes.len();

    // Pass the flag to print_header
    print_header(
        header,
        file_count,
        total_removed,
        total_added,
        show_full_desc,
    );

    if changes.is_empty() {
        return;
    }

    let max_digits_len = max_change_count.to_string().len();
    let static_overhead = 5;

    let term_width = if let Some((Width(w), _)) = terminal_size() {
        w as usize
    } else {
        80
    };

    let reserved_width = static_overhead + max_digits_len + MIN_GRAPH_WIDTH;
    let max_allowed_left_width = term_width.saturating_sub(reserved_width);

    let file_lines: Vec<(String, &FileStatus, u32, u32, u32)> = changes
        .iter()
        .map(|s| {
            let symbol = get_status_symbol(s);
            let path = get_path_display(s);
            let (added, removed) = extract_stats(s);
            let total = added + removed;

            let mut left_str = format!("{} {}", symbol, path);

            if left_str.chars().count() > max_allowed_left_width {
                let cut_point = max_allowed_left_width.saturating_sub(1);
                if cut_point > 0 {
                    let mut chars: Vec<char> = left_str.chars().collect();
                    chars.truncate(cut_point);
                    left_str = chars.into_iter().collect();
                } else {
                    left_str.clear();
                }
                left_str.push('…');
            }

            (left_str, s, added, removed, total)
        })
        .collect();

    let actual_max_left_len = file_lines
        .iter()
        .map(|(s, _, _, _, _)| s.chars().count())
        .max()
        .unwrap_or(0);
    let used_width_without_graph = actual_max_left_len + static_overhead + max_digits_len;
    let available_graph_width = term_width.saturating_sub(used_width_without_graph) as u32;

    let clamped_graph_width = if available_graph_width > 42 {
        42
    } else {
        available_graph_width
    };

    for (left_str, status, added, removed, total) in file_lines {
        let graph = build_bar_string(added, removed, total, max_change_count, clamped_graph_width);

        let colored_left = match status {
            FileStatus::Added(..) => left_str.green(),
            FileStatus::Deleted(..) => left_str.red(),
            FileStatus::Modified(..) => left_str.cyan(),
            FileStatus::Renamed { .. } => left_str.yellow(),
            FileStatus::Copied { .. } => left_str.yellow(),
        };

        println!(
            "{:width$} {} {:>digits$} {}",
            colored_left,
            "|".bright_black(),
            total.to_string().bold(),
            graph,
            width = actual_max_left_len,
            digits = max_digits_len
        );
    }
}

fn build_bar_string(
    added: u32,
    removed: u32,
    total: u32,
    global_max: u32,
    max_width: u32,
) -> String {
    if total == 0 {
        return "".to_string();
    }

    let scale_factor = if global_max > 0 {
        max_width as f64 / global_max as f64
    } else {
        1.0
    };

    let mut bar_width = (total as f64 * scale_factor).round() as u32;
    if bar_width == 0 && total > 0 {
        bar_width = 1;
    }
    if bar_width > max_width {
        bar_width = max_width;
    }

    let add_ratio = added as f64 / total as f64;
    let mut num_plus = (bar_width as f64 * add_ratio).round() as u32;
    let mut num_minus = bar_width.saturating_sub(num_plus);

    if added > 0 && removed > 0 {
        if num_plus == 0 && bar_width > 0 {
            num_plus = 1;
            num_minus = bar_width.saturating_sub(1);
        }
        if num_minus == 0 && bar_width > 0 {
            num_minus = 1;
        }
    }

    let mut result = String::with_capacity(bar_width as usize * 3);
    for _ in 0..num_plus {
        result.push_str(&"+".green().to_string());
    }
    for _ in 0..num_minus {
        result.push_str(&"-".red().to_string());
    }
    result
}

fn print_header(header: &CommitInfo, count: usize, removed: u32, added: u32, show_full_desc: bool) {
    let full_desc = &header.description;

    // Split subject (first line) and body
    let subject = full_desc.lines().next().unwrap_or("").trim();
    // Body is everything after the first newline
    let body = if let Some(idx) = full_desc.find('\n') {
        &full_desc[idx + 1..]
    } else {
        ""
    };

    let subject_display = if subject.is_empty() {
        "(no description set)".cyan()
    } else {
        subject.normal()
    };

    let ch_rest = header
        .change_id_short
        .strip_prefix(&header.change_id_prefix)
        .unwrap_or("");
    let co_rest = header
        .commit_id_short
        .strip_prefix(&header.commit_id_prefix)
        .unwrap_or("");

    let stats_block = format!(
        "{}{}{}{}{}",
        "[".bright_black(),
        count.to_string().bold(),
        format!(" -{}", removed).red(),
        format!(" +{}", added).green(),
        "]".bright_black()
    );

    println!(
        "{}{} {}{} {} {}",
        header.change_id_prefix.magenta().bold(),
        ch_rest.bright_black(),
        header.commit_id_prefix.blue().bold(),
        co_rest.bright_black(),
        subject_display,
        stats_block
    );

    if show_full_desc && !body.is_empty() {
        // Print body with slight indentation or distinction if needed.
        // Here we print it raw as requested, but ensuring it starts on a new line is handled by println.
        println!("{}", body);
    }
}

fn extract_stats(status: &FileStatus) -> (u32, u32) {
    match status {
        FileStatus::Added(_, a) => (*a, 0),
        FileStatus::Deleted(_, r) => (0, *r),
        FileStatus::Modified(_, a, r) => (*a, *r),
        FileStatus::Renamed { added, removed, .. } => (*added, *removed),
        FileStatus::Copied { added, removed, .. } => (*added, *removed),
    }
}

fn get_path_display(status: &FileStatus) -> String {
    match status {
        FileStatus::Added(p, _) | FileStatus::Deleted(p, _) | FileStatus::Modified(p, _, _) => {
            p.clone()
        }
        FileStatus::Renamed { old, new, .. } => format!("{{{} => {}}}", old, new),
        FileStatus::Copied { src, dest, .. } => format!("{{{} => {}}}", src, dest),
    }
}

fn get_status_symbol(status: &FileStatus) -> &'static str {
    match status {
        FileStatus::Added(..) => "A",
        FileStatus::Deleted(..) => "D",
        FileStatus::Modified(..) => "M",
        FileStatus::Renamed { .. } => "R",
        FileStatus::Copied { .. } => "C",
    }
}
