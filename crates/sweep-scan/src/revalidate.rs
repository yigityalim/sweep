use std::{
    fs, io,
    path::{Path, PathBuf},
};

use sweep_core::{Candidate, FileIdentity, Plan, PlanError};

use crate::size;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevalidationStatus {
    Unchanged,
    Changed,
    Missing,
    Unverifiable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RevalidationReason {
    CandidateMissing,
    CandidateBecameSymlink,
    PhysicalParentOutsideRoot,
    PhysicalContainmentUnavailable(String),
    IdentityChanged,
    IdentityUnavailable(String),
    TraversalIncomplete,
    FingerprintUnavailable,
    FingerprintChanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateRevalidation {
    pub path: PathBuf,
    pub status: RevalidationStatus,
    pub reasons: Vec<RevalidationReason>,
    pub observed_identity: Option<FileIdentity>,
    pub observed_subtree_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanRevalidation {
    pub root: PathBuf,
    pub plan_created_unix_seconds: u64,
    pub all_unchanged: bool,
    pub candidates: Vec<CandidateRevalidation>,
}

pub fn revalidate_plan(plan: &Plan) -> Result<PlanRevalidation, PlanError> {
    plan.validate()?;

    let candidates: Vec<_> = plan
        .candidates
        .iter()
        .map(|candidate| revalidate_candidate(&plan.root, candidate))
        .collect();
    let all_unchanged = candidates
        .iter()
        .all(|candidate| candidate.status == RevalidationStatus::Unchanged);

    Ok(PlanRevalidation {
        root: plan.root.clone(),
        plan_created_unix_seconds: plan.created_unix_seconds,
        all_unchanged,
        candidates,
    })
}

fn revalidate_candidate(root: &Path, candidate: &Candidate) -> CandidateRevalidation {
    let mut result = CandidateRevalidation {
        path: candidate.path.clone(),
        status: RevalidationStatus::Unchanged,
        reasons: Vec::new(),
        observed_identity: None,
        observed_subtree_fingerprint: None,
    };

    let metadata = match fs::symlink_metadata(&candidate.path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            result.status = RevalidationStatus::Missing;
            result.reasons.push(RevalidationReason::CandidateMissing);
            return result;
        }
        Err(error) => {
            result.status = RevalidationStatus::Unverifiable;
            result
                .reasons
                .push(RevalidationReason::IdentityUnavailable(error.to_string()));
            return result;
        }
    };

    if metadata.file_type().is_symlink() {
        result.status = RevalidationStatus::Changed;
        result
            .reasons
            .push(RevalidationReason::CandidateBecameSymlink);
        return result;
    }

    match physical_parent_within_root(root, &candidate.path) {
        Ok(true) => {}
        Ok(false) => {
            result.status = RevalidationStatus::Changed;
            result
                .reasons
                .push(RevalidationReason::PhysicalParentOutsideRoot);
            return result;
        }
        Err(error) => {
            result.status = RevalidationStatus::Unverifiable;
            result
                .reasons
                .push(RevalidationReason::PhysicalContainmentUnavailable(
                    error.to_string(),
                ));
            return result;
        }
    }

    let observed_identity = match FileIdentity::from_metadata(&metadata) {
        Ok(identity) => identity,
        Err(error) => {
            result.status = RevalidationStatus::Unverifiable;
            result
                .reasons
                .push(RevalidationReason::IdentityUnavailable(error.to_string()));
            return result;
        }
    };
    let identity_matches = candidate
        .identity
        .as_ref()
        .is_some_and(|identity| *identity == observed_identity);
    result.observed_identity = Some(observed_identity);

    if !identity_matches {
        result.status = RevalidationStatus::Changed;
        result.reasons.push(RevalidationReason::IdentityChanged);
    }

    let measured = size::measure(&candidate.path);
    result.observed_subtree_fingerprint = measured.subtree_metadata_fingerprint.clone();

    if !measured.traversal_complete {
        if result.status == RevalidationStatus::Unchanged {
            result.status = RevalidationStatus::Unverifiable;
        }
        result.reasons.push(RevalidationReason::TraversalIncomplete);
        return result;
    }

    let Some(observed_fingerprint) = measured.subtree_metadata_fingerprint.as_deref() else {
        if result.status == RevalidationStatus::Unchanged {
            result.status = RevalidationStatus::Unverifiable;
        }
        result
            .reasons
            .push(RevalidationReason::FingerprintUnavailable);
        return result;
    };

    if candidate.subtree_metadata_fingerprint.as_deref() != Some(observed_fingerprint) {
        result.status = RevalidationStatus::Changed;
        result.reasons.push(RevalidationReason::FingerprintChanged);
    }

    result
}

fn physical_parent_within_root(root: &Path, candidate: &Path) -> io::Result<bool> {
    let root = fs::canonicalize(root)?;
    if candidate == root {
        return Ok(true);
    }

    let parent = candidate
        .parent()
        .ok_or_else(|| io::Error::other("candidate has no parent directory"))?;
    let parent = fs::canonicalize(parent)?;
    Ok(parent.starts_with(root))
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink};

    use sweep_core::{CandidateKind, Decision, RecoveryContract, RecoveryKind};
    use tempfile::tempdir;

    use super::*;

    fn candidate(path: &Path) -> Candidate {
        let measured = size::measure(path);
        Candidate {
            path: path.to_path_buf(),
            kind: CandidateKind::RustTarget,
            decision: Decision::Safe,
            logical_bytes: measured.logical_bytes,
            allocated_bytes_estimate: measured.allocated_bytes_estimate,
            traversal_complete: measured.traversal_complete,
            subtree_metadata_fingerprint: measured.subtree_metadata_fingerprint,
            identity: FileIdentity::from_path(path).ok(),
            recovery: RecoveryContract {
                kind: RecoveryKind::Rebuild,
                command: Some(String::from("cargo build")),
                detail: String::from("rebuild"),
            },
            evidence: Vec::new(),
        }
    }

    #[test]
    fn unchanged_candidate_passes_revalidation() {
        let root = tempdir().unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("artifact"), "one").unwrap();
        let plan = Plan::from_candidates(root.path().to_path_buf(), vec![candidate(&target)]);

        let result = revalidate_plan(&plan).unwrap();

        assert!(result.all_unchanged);
        assert_eq!(result.candidates[0].status, RevalidationStatus::Unchanged);
        assert!(result.candidates[0].reasons.is_empty());
    }

    #[test]
    fn descendant_change_fails_fingerprint_revalidation() {
        let root = tempdir().unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("artifact"), "one").unwrap();
        let plan = Plan::from_candidates(root.path().to_path_buf(), vec![candidate(&target)]);

        fs::write(target.join("artifact"), "different-length").unwrap();

        let result = revalidate_plan(&plan).unwrap();

        assert!(!result.all_unchanged);
        assert_eq!(result.candidates[0].status, RevalidationStatus::Changed);
        assert!(
            result.candidates[0]
                .reasons
                .contains(&RevalidationReason::FingerprintChanged)
        );
    }

    #[test]
    fn missing_candidate_is_reported_without_discovery() {
        let root = tempdir().unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let plan = Plan::from_candidates(root.path().to_path_buf(), vec![candidate(&target)]);

        fs::remove_dir(&target).unwrap();

        let result = revalidate_plan(&plan).unwrap();

        assert_eq!(result.candidates[0].status, RevalidationStatus::Missing);
        assert_eq!(
            result.candidates[0].reasons,
            vec![RevalidationReason::CandidateMissing]
        );
    }

    #[test]
    fn symlink_replacement_is_reported_as_changed() {
        let root = tempdir().unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let plan = Plan::from_candidates(root.path().to_path_buf(), vec![candidate(&target)]);

        let replacement = root.path().join("replacement");
        fs::create_dir(&replacement).unwrap();
        fs::remove_dir(&target).unwrap();
        symlink(&replacement, &target).unwrap();

        let result = revalidate_plan(&plan).unwrap();

        assert_eq!(result.candidates[0].status, RevalidationStatus::Changed);
        assert_eq!(
            result.candidates[0].reasons,
            vec![RevalidationReason::CandidateBecameSymlink]
        );
    }
}
