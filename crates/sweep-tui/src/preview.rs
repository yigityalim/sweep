use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use sweep_report::{OutputFormat, Report, Snapshot, SnapshotDiff, render};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BrowseEntry {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) is_dir: bool,
    pub(crate) is_symlink: bool,
    pub(crate) bytes: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct GrowthData {
    pub(crate) diff: Option<SnapshotDiff>,
    pub(crate) snapshot_count: usize,
    pub(crate) invalid_snapshot_count: usize,
    pub(crate) message: String,
}

pub(crate) fn list_directory(path: &Path) -> io::Result<Vec<BrowseEntry>> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let is_dir = file_type.is_dir();
        let is_symlink = file_type.is_symlink();
        let bytes = if file_type.is_file() {
            Some(entry.metadata()?.len())
        } else {
            None
        };

        entries.push(BrowseEntry {
            path: entry.path(),
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir,
            is_symlink,
            bytes,
        });
    }

    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });

    Ok(entries)
}

pub(crate) fn reveal_in_finder(path: &Path) -> io::Result<()> {
    if env::consts::OS != "macos" {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Finder reveal is currently supported on macOS only",
        ));
    }

    let status = Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("open -R exited with {status}")))
    }
}

pub(crate) fn save_report(report: &Report, format: OutputFormat) -> io::Result<PathBuf> {
    let rendered = render(report, format).map_err(io::Error::other)?;
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    let downloads = home.join("Downloads");
    fs::create_dir_all(&downloads)?;

    let stem = format!("sweep-report-{}", report.created_unix_seconds);
    let output = unique_path(&downloads, &stem, format.extension());
    atomic_write(&output, rendered.as_bytes())?;
    Ok(output)
}

pub(crate) fn copy_text_report(report: &Report) -> io::Result<()> {
    if env::consts::OS != "macos" {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "clipboard copy is currently supported on macOS only",
        ));
    }

    let rendered = render(report, OutputFormat::Text).map_err(io::Error::other)?;
    let mut child = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let Some(mut stdin) = child.stdin.take() else {
        return Err(io::Error::other("could not open pbcopy stdin"));
    };

    stdin.write_all(rendered.as_bytes())?;
    drop(stdin);

    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("pbcopy exited with {status}")))
    }
}

pub(crate) fn load_growth(root: &Path) -> io::Result<GrowthData> {
    let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
        return Ok(GrowthData {
            diff: None,
            snapshot_count: 0,
            invalid_snapshot_count: 0,
            message: String::from("HOME is not set; snapshots cannot be discovered"),
        });
    };

    let directory = home
        .join("Library")
        .join("Application Support")
        .join("Sweep")
        .join("snapshots");

    if !directory.is_dir() {
        return Ok(GrowthData {
            diff: None,
            snapshot_count: 0,
            invalid_snapshot_count: 0,
            message: String::from("No snapshots yet. Run sw snapshot for this scope twice."),
        });
    }

    let expected_root = root.to_string_lossy();
    let mut snapshots = Vec::new();
    let mut invalid_snapshot_count = 0usize;

    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with("sweep-snapshot-") || !file_name.ends_with(".sweep.json") {
            continue;
        }

        let content = match fs::read_to_string(entry.path()) {
            Ok(content) => content,
            Err(_) => {
                invalid_snapshot_count += 1;
                continue;
            }
        };

        let snapshot = match serde_json::from_str::<Snapshot>(&content) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                invalid_snapshot_count += 1;
                continue;
            }
        };

        if snapshot.validate().is_err() {
            invalid_snapshot_count += 1;
            continue;
        }

        if snapshot.root == expected_root {
            snapshots.push(snapshot);
        }
    }

    snapshots.sort_by_key(|snapshot| snapshot.created_unix_seconds);
    let snapshot_count = snapshots.len();

    if snapshot_count < 2 {
        let suffix = invalid_suffix(invalid_snapshot_count);
        return Ok(GrowthData {
            diff: None,
            snapshot_count,
            invalid_snapshot_count,
            message: format!(
                "Need two valid snapshots for this scope; found {snapshot_count}.{suffix}"
            ),
        });
    }

    let before = &snapshots[snapshot_count - 2];
    let after = &snapshots[snapshot_count - 1];
    let diff = before.diff(after).map_err(io::Error::other)?;
    let suffix = invalid_suffix(invalid_snapshot_count);

    Ok(GrowthData {
        diff: Some(diff),
        snapshot_count,
        invalid_snapshot_count,
        message: format!("Comparing the latest two of {snapshot_count} snapshots.{suffix}"),
    })
}

pub(crate) fn history_directory() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from).map(|home| {
        home.join("Library")
            .join("Application Support")
            .join("Sweep")
            .join("history")
    })
}

fn invalid_suffix(count: usize) -> String {
    if count == 0 {
        String::new()
    } else {
        format!(" {count} invalid snapshot file(s) ignored.")
    }
}

fn unique_path(directory: &Path, stem: &str, extension: &str) -> PathBuf {
    let direct = directory.join(format!("{stem}.{extension}"));
    if !direct.exists() {
        return direct;
    }

    for suffix in 1_u32.. {
        let candidate = directory.join(format!("{stem}-{suffix}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("u32 report filename suffix space exhausted")
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("sweep-report"),
        std::process::id(),
        nonce
    ));

    let mut file = fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_path_does_not_clobber_existing_file() {
        let base = env::temp_dir().join(format!(
            "sweep-preview-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&base).unwrap();
        let first = base.join("report.md");
        fs::write(&first, b"existing").unwrap();

        assert_eq!(unique_path(&base, "report", "md"), base.join("report-1.md"));

        fs::remove_dir_all(base).unwrap();
    }
}
