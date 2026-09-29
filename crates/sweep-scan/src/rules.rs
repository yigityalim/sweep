use std::{fs, path::Path};

use sweep_core::{Candidate, CandidateKind, Decision, Evidence, FileIdentity, RecoveryContract};

use crate::{
    git::{self, GitState},
    providers::{self, GitEvidencePolicy},
    size,
};

pub fn classify_path(path: &Path) -> Candidate {
    let canonical_display = path.to_path_buf();

    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Candidate {
            path: canonical_display,
            kind: CandidateKind::Unrecognized,
            decision: Decision::Protected,
            logical_bytes: 0,
            allocated_bytes_estimate: 0,
            traversal_complete: false,
            subtree_metadata_fingerprint: None,
            identity: FileIdentity::from_path(path).ok(),
            recovery: RecoveryContract::none("Symlink candidates are never followed or deleted."),
            evidence: vec![Evidence::refuted(
                "candidate_not_symlink",
                "The requested path is a symlink.",
            )],
        };
    }

    let Some(mut assessment) = providers::assess(path) else {
        return Candidate {
            path: canonical_display,
            kind: CandidateKind::Unrecognized,
            decision: Decision::Protected,
            logical_bytes: 0,
            allocated_bytes_estimate: 0,
            traversal_complete: false,
            subtree_metadata_fingerprint: None,
            identity: FileIdentity::from_path(path).ok(),
            recovery: RecoveryContract::none(
                "No supported recovery contract exists for this path.",
            ),
            evidence: vec![Evidence::refuted(
                "supported_rule",
                "No provider-specific rule recognizes this path.",
            )],
        };
    };

    let size = size::measure(path);

    if !size.traversal_complete {
        assessment.decision = assessment.decision.max(Decision::Review);
        assessment.evidence.push(Evidence::unknown(
            "complete_traversal",
            "At least one entry could not be measured.",
        ));
    } else {
        assessment.evidence.push(Evidence::proven(
            "complete_traversal",
            "Candidate traversal completed without a filesystem read error.",
        ));
    }

    if size.nested_repository {
        assessment.decision = Decision::Protected;
        assessment.evidence.push(Evidence::refuted(
            "nested_repository",
            "A nested .git boundary exists inside the candidate.",
        ));
    } else {
        assessment.evidence.push(Evidence::proven(
            "nested_repository",
            "No nested .git boundary was observed.",
        ));
    }

    apply_git_evidence(path, &mut assessment);

    Candidate {
        path: canonical_display,
        kind: assessment.kind,
        decision: assessment.decision,
        logical_bytes: size.logical_bytes,
        allocated_bytes_estimate: size.allocated_bytes_estimate,
        traversal_complete: size.traversal_complete,
        subtree_metadata_fingerprint: size.subtree_metadata_fingerprint,
        identity: FileIdentity::from_path(path).ok(),
        recovery: assessment.recovery,
        evidence: assessment.evidence,
    }
}

fn apply_git_evidence(path: &Path, assessment: &mut providers::ProviderAssessment) {
    if assessment.git_evidence == GitEvidencePolicy::NotApplicable {
        assessment.evidence.push(Evidence::proven(
            "git_ownership",
            "This provider boundary is independent of an enclosing Git repository.",
        ));
        return;
    }

    match git::inspect(path) {
        GitState::IgnoredAndUntracked => {
            assessment.evidence.push(Evidence::proven(
                "git_ownership",
                "Git reports no tracked candidate path and reports the candidate as ignored.",
            ));
        }
        GitState::Tracked => {
            assessment.decision = Decision::Protected;
            assessment.evidence.push(Evidence::refuted(
                "git_ownership",
                "Git reports tracked content at this candidate path.",
            ));
        }
        GitState::NotIgnored => {
            assessment.decision = assessment.decision.max(Decision::Review);
            assessment.evidence.push(Evidence::unknown(
                "git_ownership",
                "The candidate is not ignored by Git.",
            ));
        }
        GitState::NotRepository => {
            assessment.decision = assessment.decision.max(Decision::Review);
            assessment.evidence.push(Evidence::unknown(
                "git_ownership",
                "No enclosing Git repository was found.",
            ));
        }
        GitState::Unavailable(error) => {
            assessment.decision = assessment.decision.max(Decision::Review);
            assessment
                .evidence
                .push(Evidence::unknown("git_ownership", error));
        }
    }
}
