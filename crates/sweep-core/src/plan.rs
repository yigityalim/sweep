use std::{
    collections::BTreeSet,
    error::Error,
    fmt,
    path::{Component, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{Candidate, Decision, RecoveryKind};

pub const PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub schema_version: u32,
    pub created_unix_seconds: u64,
    pub root: PathBuf,
    pub candidates: Vec<Candidate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanError {
    UnsupportedSchemaVersion(u32),
    RootNotAbsolute(PathBuf),
    CandidateNotSafe(PathBuf),
    CandidateOutsideRoot(PathBuf),
    InvalidCandidatePath(PathBuf),
    DuplicateCandidate(PathBuf),
    IncompleteTraversal(PathBuf),
    MissingIdentity(PathBuf),
    MissingSubtreeFingerprint(PathBuf),
    MissingRecoveryContract(PathBuf),
}

impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "unsupported plan schema version {version}")
            }
            Self::RootNotAbsolute(root) => {
                write!(formatter, "plan root is not absolute: {}", root.display())
            }
            Self::CandidateNotSafe(path) => {
                write!(
                    formatter,
                    "plan contains a non-safe candidate: {}",
                    path.display()
                )
            }
            Self::CandidateOutsideRoot(path) => {
                write!(
                    formatter,
                    "plan candidate is outside the declared root: {}",
                    path.display()
                )
            }
            Self::InvalidCandidatePath(path) => {
                write!(
                    formatter,
                    "plan candidate path is not normalized: {}",
                    path.display()
                )
            }
            Self::DuplicateCandidate(path) => {
                write!(
                    formatter,
                    "plan contains a duplicate candidate: {}",
                    path.display()
                )
            }
            Self::IncompleteTraversal(path) => {
                write!(
                    formatter,
                    "plan candidate traversal was incomplete: {}",
                    path.display()
                )
            }
            Self::MissingIdentity(path) => {
                write!(
                    formatter,
                    "plan candidate has no filesystem identity: {}",
                    path.display()
                )
            }
            Self::MissingSubtreeFingerprint(path) => {
                write!(
                    formatter,
                    "plan candidate has no subtree fingerprint: {}",
                    path.display()
                )
            }
            Self::MissingRecoveryContract(path) => {
                write!(
                    formatter,
                    "plan candidate has no recovery contract: {}",
                    path.display()
                )
            }
        }
    }
}

impl Error for PlanError {}

impl Plan {
    pub fn from_candidates(root: PathBuf, candidates: Vec<Candidate>) -> Self {
        let created_unix_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let candidates = candidates
            .into_iter()
            .filter(|candidate| candidate.decision == Decision::Safe)
            .collect();

        Self {
            schema_version: PLAN_SCHEMA_VERSION,
            created_unix_seconds,
            root,
            candidates,
        }
    }

    pub fn validate(&self) -> Result<(), PlanError> {
        if self.schema_version != PLAN_SCHEMA_VERSION {
            return Err(PlanError::UnsupportedSchemaVersion(self.schema_version));
        }
        if !self.root.is_absolute() {
            return Err(PlanError::RootNotAbsolute(self.root.clone()));
        }

        let mut seen = BTreeSet::new();
        for candidate in &self.candidates {
            if candidate.decision != Decision::Safe {
                return Err(PlanError::CandidateNotSafe(candidate.path.clone()));
            }

            let relative = candidate
                .path
                .strip_prefix(&self.root)
                .map_err(|_| PlanError::CandidateOutsideRoot(candidate.path.clone()))?;
            if relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(PlanError::InvalidCandidatePath(candidate.path.clone()));
            }

            if !seen.insert(candidate.path.clone()) {
                return Err(PlanError::DuplicateCandidate(candidate.path.clone()));
            }
            if !candidate.traversal_complete {
                return Err(PlanError::IncompleteTraversal(candidate.path.clone()));
            }
            if candidate.identity.is_none() {
                return Err(PlanError::MissingIdentity(candidate.path.clone()));
            }
            if candidate.subtree_metadata_fingerprint.is_none() {
                return Err(PlanError::MissingSubtreeFingerprint(candidate.path.clone()));
            }
            if candidate.recovery.kind == RecoveryKind::None {
                return Err(PlanError::MissingRecoveryContract(candidate.path.clone()));
            }
        }

        Ok(())
    }

    pub fn allocated_bytes_estimate(&self) -> u64 {
        self.candidates
            .iter()
            .map(|candidate| candidate.allocated_bytes_estimate)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CandidateKind, Evidence, FileIdentity, RecoveryContract, RecoveryKind};

    fn candidate(path: &str, decision: Decision) -> Candidate {
        Candidate {
            path: PathBuf::from(path),
            kind: CandidateKind::NodeModules,
            decision,
            logical_bytes: 10,
            allocated_bytes_estimate: 8,
            traversal_complete: true,
            subtree_metadata_fingerprint: Some(String::from("fingerprint")),
            identity: Some(FileIdentity {
                device: 1,
                inode: 2,
                size: 3,
                modified_seconds: 4,
                modified_nanoseconds: 5,
            }),
            recovery: RecoveryContract {
                kind: RecoveryKind::ReinstallDependencies,
                command: None,
                detail: String::from("reinstall"),
            },
            evidence: vec![Evidence::proven("test", "test")],
        }
    }

    #[test]
    fn plan_contains_only_safe_candidates() {
        let plan = Plan::from_candidates(
            PathBuf::from("/tmp"),
            vec![
                candidate("/tmp/a", Decision::Safe),
                candidate("/tmp/b", Decision::Review),
                candidate("/tmp/c", Decision::Protected),
            ],
        );

        assert_eq!(plan.candidates.len(), 1);
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn validation_rejects_candidate_outside_root() {
        let plan = Plan::from_candidates(
            PathBuf::from("/tmp/root"),
            vec![candidate("/tmp/other/a", Decision::Safe)],
        );

        assert!(matches!(
            plan.validate(),
            Err(PlanError::CandidateOutsideRoot(_))
        ));
    }

    #[test]
    fn validation_rejects_missing_identity() {
        let mut item = candidate("/tmp/a", Decision::Safe);
        item.identity = None;
        let plan = Plan::from_candidates(PathBuf::from("/tmp"), vec![item]);

        assert!(matches!(
            plan.validate(),
            Err(PlanError::MissingIdentity(_))
        ));
    }

    #[test]
    fn validation_rejects_missing_fingerprint() {
        let mut item = candidate("/tmp/a", Decision::Safe);
        item.subtree_metadata_fingerprint = None;
        let plan = Plan::from_candidates(PathBuf::from("/tmp"), vec![item]);

        assert!(matches!(
            plan.validate(),
            Err(PlanError::MissingSubtreeFingerprint(_))
        ));
    }

    #[test]
    fn validation_rejects_duplicate_path() {
        let plan = Plan::from_candidates(
            PathBuf::from("/tmp"),
            vec![
                candidate("/tmp/a", Decision::Safe),
                candidate("/tmp/a", Decision::Safe),
            ],
        );

        assert!(matches!(
            plan.validate(),
            Err(PlanError::DuplicateCandidate(_))
        ));
    }
}
