use serde::Serialize;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileStatus {
    Added(String, u32),
    Deleted(String, u32),
    Modified(String, u32, u32),
    Renamed {
        old: String,
        new: String,
        added: u32,
        removed: u32,
    },
    Copied {
        src: String,
        dest: String,
        added: u32,
        removed: u32,
    },
}

#[derive(Debug, Serialize)]
pub struct CommitInfo {
    pub description: String,

    // Change ID parts
    pub change_id_prefix: String, // e.g. "u"
    pub change_id_short: String,  // e.g. "ustt"
    pub change_id_full: String,   // e.g. "ustt..." (full length)

    // Commit ID parts
    pub commit_id_prefix: String,
    pub commit_id_short: String,
    pub commit_id_full: String,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

// --- JSON Internal Structures ---

#[derive(Serialize)]
struct JsonRoot {
    target: JsonTarget,
    files: Vec<JsonFileEntry>,
}

#[derive(Serialize)]
struct JsonTarget {
    from: JsonRevisionInfo,
    into: JsonRevisionInfo,
}

#[derive(Serialize)]
struct JsonRevisionInfo {
    description: String,
    // [prefix, rest_of_full_id]
    change_id: Vec<String>,
    commit_id: Vec<String>,
}

#[derive(Serialize)]
struct JsonFileEntry {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    added: u32,
    removed: u32,
}

impl From<&CommitInfo> for JsonRevisionInfo {
    fn from(info: &CommitInfo) -> Self {
        // For JSON, we use the FULL ID, split by the unique prefix
        let ch_rest = info
            .change_id_full
            .strip_prefix(&info.change_id_prefix)
            .unwrap_or("");
        let co_rest = info
            .commit_id_full
            .strip_prefix(&info.commit_id_prefix)
            .unwrap_or("");

        Self {
            description: info.description.clone(),
            change_id: vec![info.change_id_prefix.clone(), ch_rest.to_string()],
            commit_id: vec![info.commit_id_prefix.clone(), co_rest.to_string()],
        }
    }
}

impl From<&FileStatus> for JsonFileEntry {
    fn from(status: &FileStatus) -> Self {
        match status {
            FileStatus::Added(p, a) => Self {
                status: "A".to_string(),
                path: Some(p.clone()),
                from: None,
                to: None,
                added: *a,
                removed: 0,
            },
            FileStatus::Deleted(p, r) => Self {
                status: "D".to_string(),
                path: Some(p.clone()),
                from: None,
                to: None,
                added: 0,
                removed: *r,
            },
            FileStatus::Modified(p, a, r) => Self {
                status: "M".to_string(),
                path: Some(p.clone()),
                from: None,
                to: None,
                added: *a,
                removed: *r,
            },
            FileStatus::Renamed {
                old,
                new,
                added,
                removed,
            } => Self {
                status: "R".to_string(),
                path: None,
                from: Some(old.clone()),
                to: Some(new.clone()),
                added: *added,
                removed: *removed,
            },
            FileStatus::Copied {
                src,
                dest,
                added,
                removed,
            } => Self {
                status: "C".to_string(),
                path: None,
                from: Some(src.clone()),
                to: Some(dest.clone()),
                added: *added,
                removed: *removed,
            },
        }
    }
}

pub fn print_json(
    from_info: &CommitInfo,
    to_info: &CommitInfo,
    changes: &[FileStatus],
) -> color_eyre::Result<()> {
    let root = JsonRoot {
        target: JsonTarget {
            from: JsonRevisionInfo::from(from_info),
            into: JsonRevisionInfo::from(to_info),
        },
        files: changes.iter().map(JsonFileEntry::from).collect(),
    };

    println!("{}", serde_json::to_string_pretty(&root)?);
    Ok(())
}
