use std::path::Path;

use sweep_core::{CandidateKind, RecoveryContract, RecoveryKind};

use super::{GitEvidencePolicy, Provider, ProviderAssessment, ProviderContext};

pub struct GoProvider;

impl Provider for GoProvider {
    fn assess(&self, path: &Path, context: &ProviderContext) -> Option<ProviderAssessment> {
        let home = context.home()?;

        if path == home.join("Library/Caches/go-build") {
            let mut assessment = ProviderAssessment::new(
                CandidateKind::GoBuildCache,
                RecoveryContract {
                    kind: RecoveryKind::Rebuild,
                    command: None,
                    detail: String::from(
                        "Go build-cache entries are compiler outputs regenerated on demand by the Go toolchain.",
                    ),
                },
                GitEvidencePolicy::NotApplicable,
                "default macOS GOCACHE root",
            );
            assessment.prove(
                "owner_managed_cache",
                "The path is the default macOS Go build-cache root. Future mutation must delegate to `go clean -cache` instead of recursively deleting arbitrary paths.",
            );
            return Some(assessment);
        }

        if path == home.join("go/pkg/mod") {
            let mut assessment = ProviderAssessment::new(
                CandidateKind::GoModuleCache,
                RecoveryContract {
                    kind: RecoveryKind::ReinstallDependencies,
                    command: None,
                    detail: String::from(
                        "The Go module cache contains downloaded versioned dependency source. Re-download may depend on module availability, network access, and private-module credentials.",
                    ),
                },
                GitEvidencePolicy::NotApplicable,
                "default GOPATH module-cache root",
            );
            assessment.review(
                "module_source_recovery",
                "Exact recovery of every cached module is not proven, so the module cache is review-only.",
            );
            return Some(assessment);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sweep_core::Decision;

    use super::*;

    #[test]
    fn default_macos_build_cache_is_safe() {
        let home = PathBuf::from("/Users/example");
        let context = ProviderContext {
            home: Some(home.clone()),
        };
        let path = home.join("Library/Caches/go-build");

        let assessment = GoProvider.assess(&path, &context).unwrap();

        assert_eq!(assessment.kind, CandidateKind::GoBuildCache);
        assert_eq!(assessment.decision, Decision::Safe);
        assert_eq!(assessment.git_evidence, GitEvidencePolicy::NotApplicable);
    }

    #[test]
    fn default_module_cache_is_review_only() {
        let home = PathBuf::from("/Users/example");
        let context = ProviderContext {
            home: Some(home.clone()),
        };
        let path = home.join("go/pkg/mod");

        let assessment = GoProvider.assess(&path, &context).unwrap();

        assert_eq!(assessment.kind, CandidateKind::GoModuleCache);
        assert_eq!(assessment.decision, Decision::Review);
        assert_eq!(assessment.git_evidence, GitEvidencePolicy::NotApplicable);
    }
}
