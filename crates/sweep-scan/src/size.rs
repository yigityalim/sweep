use std::{collections::HashSet, fs, path::Path};

use blake3::Hasher;
use ignore::WalkBuilder;

#[cfg(unix)]
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SizeReport {
    pub logical_bytes: u64,
    pub allocated_bytes_estimate: u64,
    pub traversal_complete: bool,
    pub nested_repository: bool,
    pub subtree_metadata_fingerprint: Option<String>,
}

pub fn measure(path: &Path) -> SizeReport {
    let mut report = SizeReport {
        traversal_complete: true,
        ..SizeReport::default()
    };
    let mut seen = HashSet::new();
    let mut entry_hashes = Vec::new();

    let walker = WalkBuilder::new(path)
        .standard_filters(false)
        .hidden(false)
        .follow_links(false)
        .same_file_system(true)
        .build();

    for result in walker {
        let entry = match result {
            Ok(entry) => entry,
            Err(_) => {
                report.traversal_complete = false;
                continue;
            }
        };

        let entry_path = entry.path();

        if entry_path != path
            && entry_path
                .file_name()
                .is_some_and(|name| name.as_bytes() == b".git")
        {
            report.nested_repository = true;
        }

        let metadata = match fs::symlink_metadata(entry_path) {
            Ok(metadata) => metadata,
            Err(_) => {
                report.traversal_complete = false;
                continue;
            }
        };

        #[cfg(unix)]
        {
            let identity = (metadata.dev(), metadata.ino());
            if metadata.nlink() > 1 && !seen.insert(identity) {
                continue;
            }

            report.logical_bytes = report.logical_bytes.saturating_add(metadata.size());
            report.allocated_bytes_estimate = report
                .allocated_bytes_estimate
                .saturating_add(metadata.blocks().saturating_mul(512));

            let mut entry_hasher = Hasher::new();
            let relative = entry_path.strip_prefix(path).unwrap_or(entry_path);
            entry_hasher.update(relative.as_os_str().as_bytes());
            entry_hasher.update(&metadata.dev().to_le_bytes());
            entry_hasher.update(&metadata.ino().to_le_bytes());
            entry_hasher.update(&metadata.mode().to_le_bytes());
            entry_hasher.update(&metadata.size().to_le_bytes());
            entry_hasher.update(&metadata.mtime().to_le_bytes());
            entry_hasher.update(&metadata.mtime_nsec().to_le_bytes());
            entry_hashes.push(*entry_hasher.finalize().as_bytes());
        }

        #[cfg(not(unix))]
        {
            report.logical_bytes = report.logical_bytes.saturating_add(metadata.len());
        }
    }

    if report.traversal_complete {
        entry_hashes.sort_unstable();
        let mut tree_hasher = Hasher::new();
        for hash in entry_hashes {
            tree_hasher.update(&hash);
        }
        report.subtree_metadata_fingerprint = Some(tree_hasher.finalize().to_hex().to_string());
    }

    report
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn does_not_follow_symlinks() {
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("large"), vec![0_u8; 1024 * 1024]).unwrap();

        let inside = tempdir().unwrap();
        symlink(outside.path(), inside.path().join("external")).unwrap();

        let report = measure(inside.path());

        assert!(report.logical_bytes < 1024 * 1024);
        assert!(report.subtree_metadata_fingerprint.is_some());
    }

    #[test]
    fn fingerprint_changes_when_descendant_metadata_changes() {
        let root = tempdir().unwrap();
        let file = root.path().join("file");
        fs::write(&file, "one").unwrap();

        let before = measure(root.path()).subtree_metadata_fingerprint.unwrap();

        fs::write(&file, "different-length").unwrap();

        let after = measure(root.path()).subtree_metadata_fingerprint.unwrap();

        assert_ne!(before, after);
    }
}
