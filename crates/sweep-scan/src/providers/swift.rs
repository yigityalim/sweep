use std::path::Path;

use sweep_core::{CandidateKind, RecoveryContract, RecoveryKind};

use super::{GitEvidencePolicy, Provider, ProviderAssessment, ProviderContext};

pub struct SwiftProvider;

impl Provider for SwiftProvider {
    fn assess(&self, path: &Path, _context: &ProviderContext) -> Option<ProviderAssessment> {
        let parent = path.parent()?;
        if path.file_name()?.to_str()? != ".build" || !parent.join("Package.swift").is_file() {
            return None;
        }

        Some(ProviderAssessment::new(
            CandidateKind::SwiftBuild,
            RecoveryContract {
                kind: RecoveryKind::Rebuild,
                command: Some(String::from("swift build")),
                detail: String::from("Swift Package Manager build state is reproducible."),
            },
            GitEvidencePolicy::Required,
            "Package.swift + .build",
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn swift_build_requires_package_manifest() {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join(".build")).unwrap();

        assert!(
            SwiftProvider
                .assess(&root.path().join(".build"), &ProviderContext { home: None },)
                .is_none()
        );

        fs::write(root.path().join("Package.swift"), "").unwrap();

        assert!(
            SwiftProvider
                .assess(&root.path().join(".build"), &ProviderContext { home: None },)
                .is_some()
        );
    }
}
