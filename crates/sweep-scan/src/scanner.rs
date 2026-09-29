use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use ignore::{WalkBuilder, WalkState};
use rayon::prelude::*;
use sweep_core::Candidate;

use crate::rules::{candidate_kind, classify_path};

#[derive(Clone, Debug)]
pub struct ScanOptions {
    pub same_file_system: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            same_file_system: true,
        }
    }
}

#[derive(Debug)]
pub enum ScanError {
    InvalidRoot(PathBuf, io::Error),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRoot(path, error) => {
                write!(formatter, "cannot scan {}: {error}", path.display())
            }
        }
    }
}

impl std::error::Error for ScanError {}

pub fn scan(root: &Path, options: &ScanOptions) -> Result<Vec<Candidate>, ScanError> {
    let root = root
        .canonicalize()
        .map_err(|error| ScanError::InvalidRoot(root.to_path_buf(), error))?;

    let paths = Arc::new(Mutex::new(Vec::<PathBuf>::new()));

    let mut builder = WalkBuilder::new(&root);
    builder
        .standard_filters(false)
        .hidden(false)
        .follow_links(false)
        .same_file_system(options.same_file_system);

    let walker = builder.build_parallel();
    walker.run(|| {
        let paths = Arc::clone(&paths);
        Box::new(move |entry| {
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };

            let Some(file_type) = entry.file_type() else {
                return WalkState::Continue;
            };

            if !file_type.is_dir() || entry.depth() == 0 {
                return WalkState::Continue;
            }

            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| matches!(name, ".git" | ".hg" | ".svn"))
            {
                return WalkState::Skip;
            }

            if candidate_kind(entry.path()).is_some() {
                if let Ok(mut paths) = paths.lock() {
                    paths.push(entry.into_path());
                }
                return WalkState::Skip;
            }

            WalkState::Continue
        })
    });

    let mut paths = Arc::try_unwrap(paths)
        .unwrap_or_else(|_| unreachable!("parallel walk must release path references"))
        .into_inner()
        .unwrap_or_default();

    paths.sort();
    paths.dedup();

    let mut candidates: Vec<_> = paths.par_iter().map(|path| classify_path(path)).collect();

    candidates.sort_by(|left, right| {
        right
            .allocated_bytes_estimate
            .cmp(&left.allocated_bytes_estimate)
            .then_with(|| left.path.cmp(&right.path))
    });

    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn accepted_candidate_stops_nested_discovery() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("package.json"), "{}").unwrap();

        let modules = root.path().join("node_modules");
        fs::create_dir_all(modules.join("nested/node_modules")).unwrap();

        let candidates = scan(root.path(), &ScanOptions::default()).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, modules.canonicalize().unwrap());
    }
}
