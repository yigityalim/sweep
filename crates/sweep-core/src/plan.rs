use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{Candidate, Decision};

pub const PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub schema_version: u32,
    pub created_unix_seconds: u64,
    pub root: PathBuf,
    pub candidates: Vec<Candidate>,
}

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
    use crate::{CandidateKind, Evidence, RecoveryContract, RecoveryKind};

    fn candidate(decision: Decision) -> Candidate {
        Candidate {
            path: PathBuf::from("/tmp/example"),
            kind: CandidateKind::NodeModules,
            decision,
            logical_bytes: 10,
            allocated_bytes_estimate: 8,
            traversal_complete: true,
            subtree_metadata_fingerprint: None,
            identity: None,
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
                candidate(Decision::Safe),
                candidate(Decision::Review),
                candidate(Decision::Protected),
            ],
        );

        assert_eq!(plan.candidates.len(), 1);
    }
}
