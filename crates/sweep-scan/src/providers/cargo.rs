use std::{fs, io, path::Path};

use sweep_core::{CandidateKind, Decision, Evidence, RecoveryContract, RecoveryKind};

use super::{GitEvidencePolicy, Provider, ProviderAssessment, ProviderContext};

pub struct CargoProvider;

impl Provider for CargoProvider {
    fn assess(&self, path: &Path, _context: &ProviderContext) -> Option<ProviderAssessment> {
        let parent = path.parent()?;
        if path.file_name()?.to_str()? != "target" || !parent.join("Cargo.toml").is_file() {
            return None;
        }

        let mut assessment = ProviderAssessment::new(
            CandidateKind::RustTarget,
            RecoveryContract {
                kind: RecoveryKind::Rebuild,
                command: Some(String::from("cargo build")),
                detail: String::from(
                    "Cargo build output is reproducible unless provider-specific protected state is present.",
                ),
            },
            GitEvidencePolicy::Required,
            "Cargo.toml + target",
        );

        let (decision, evidence) = deployment_identity_evidence(path);
        assessment.decision = assessment.decision.max(decision);
        assessment.evidence.push(evidence);

        Some(assessment)
    }
}

fn deployment_identity_evidence(target: &Path) -> (Decision, Evidence) {
    let deploy = target.join("deploy");

    let entries = match fs::read_dir(&deploy) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return (
                Decision::Safe,
                Evidence::proven(
                    "deployment_identity",
                    "No target/deploy directory exists inside the Cargo build tree.",
                ),
            );
        }
        Err(error) => {
            return (
                Decision::Review,
                Evidence::unknown(
                    "deployment_identity",
                    format!("Could not inspect target/deploy: {error}"),
                ),
            );
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                return (
                    Decision::Review,
                    Evidence::unknown(
                        "deployment_identity",
                        format!("Could not inspect an entry under target/deploy: {error}"),
                    ),
                );
            }
        };

        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.ends_with("-keypair.json"))
        {
            return (
                Decision::Protected,
                Evidence::refuted(
                    "deployment_identity",
                    "target/deploy contains an Anchor/Solana *-keypair.json deployment identity.",
                ),
            );
        }
    }

    (
        Decision::Safe,
        Evidence::proven(
            "deployment_identity",
            "No Anchor/Solana deployment keypair was found under target/deploy.",
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn anchor_deployment_keypair_protects_cargo_target() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("Cargo.toml"), "[package]\nname='demo'\n").unwrap();
        fs::create_dir_all(root.path().join("target/deploy")).unwrap();
        fs::write(
            root.path().join("target/deploy/demo-keypair.json"),
            "[1,2,3]",
        )
        .unwrap();

        let assessment = CargoProvider
            .assess(&root.path().join("target"), &ProviderContext { home: None })
            .unwrap();

        assert_eq!(assessment.decision, Decision::Protected);
    }
}
