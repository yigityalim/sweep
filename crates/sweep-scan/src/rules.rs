use std::{
    fs,
    path::{Path, PathBuf},
};

use sweep_core::{
    Candidate, CandidateKind, Decision, Evidence, FileIdentity, RecoveryContract, RecoveryKind,
};

use crate::{
    git::{self, GitState},
    size,
};

pub fn classify_path(path: &Path) -> Candidate {
    let canonical_display = path.to_path_buf();
    let mut evidence = Vec::new();

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

    let Some((kind, recovery)) = recognize(path, &mut evidence) else {
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
    let mut decision = Decision::Safe;

    if !size.traversal_complete {
        decision = decision.max(Decision::Review);
        evidence.push(Evidence::unknown(
            "complete_traversal",
            "At least one entry could not be measured.",
        ));
    } else {
        evidence.push(Evidence::proven(
            "complete_traversal",
            "Candidate traversal completed without a filesystem read error.",
        ));
    }

    if size.nested_repository {
        decision = Decision::Protected;
        evidence.push(Evidence::refuted(
            "nested_repository",
            "A nested .git boundary exists inside the candidate.",
        ));
    } else {
        evidence.push(Evidence::proven(
            "nested_repository",
            "No nested .git boundary was observed.",
        ));
    }

    if kind == CandidateKind::RustTarget && contains_anchor_keypair(path) {
        decision = Decision::Protected;
        evidence.push(Evidence::refuted(
            "deployment_identity",
            "target/deploy contains an Anchor/Solana *-keypair.json deployment identity.",
        ));
    }

    match git::inspect(path) {
        GitState::IgnoredAndUntracked => {
            evidence.push(Evidence::proven(
                "git_ownership",
                "Git reports no tracked candidate path and reports the candidate as ignored.",
            ));
        }
        GitState::Tracked => {
            decision = Decision::Protected;
            evidence.push(Evidence::refuted(
                "git_ownership",
                "Git reports tracked content at this candidate path.",
            ));
        }
        GitState::NotIgnored => {
            decision = decision.max(Decision::Review);
            evidence.push(Evidence::unknown(
                "git_ownership",
                "The candidate is not ignored by Git.",
            ));
        }
        GitState::NotRepository => {
            decision = decision.max(Decision::Review);
            evidence.push(Evidence::unknown(
                "git_ownership",
                "No enclosing Git repository was found.",
            ));
        }
        GitState::Unavailable(error) => {
            decision = decision.max(Decision::Review);
            evidence.push(Evidence::unknown("git_ownership", error));
        }
    }

    Candidate {
        path: canonical_display,
        kind,
        decision,
        logical_bytes: size.logical_bytes,
        allocated_bytes_estimate: size.allocated_bytes_estimate,
        traversal_complete: size.traversal_complete,
        subtree_metadata_fingerprint: size.subtree_metadata_fingerprint,
        identity: FileIdentity::from_path(path).ok(),
        recovery,
        evidence,
    }
}

pub fn candidate_kind(path: &Path) -> Option<CandidateKind> {
    recognize(path, &mut Vec::new()).map(|(kind, _)| kind)
}

fn recognize(
    path: &Path,
    evidence: &mut Vec<Evidence>,
) -> Option<(CandidateKind, RecoveryContract)> {
    let name = path.file_name()?.to_str()?;
    let parent = path.parent()?;

    let result = match name {
        "node_modules" if parent.join("package.json").is_file() => (
            CandidateKind::NodeModules,
            RecoveryContract {
                kind: RecoveryKind::ReinstallDependencies,
                command: node_restore_command(parent),
                detail: String::from(
                    "Dependencies can be reconstructed from the package manifest and lockfile.",
                ),
            },
            "package.json",
        ),
        ".next" if parent.join("package.json").is_file() => (
            CandidateKind::NextBuild,
            RecoveryContract {
                kind: RecoveryKind::Rebuild,
                command: Some(String::from("package-manager build")),
                detail: String::from("Next.js build output is reproducible from project sources."),
            },
            "package.json",
        ),
        ".turbo" if parent.join("package.json").is_file() => (
            CandidateKind::TurboCache,
            RecoveryContract {
                kind: RecoveryKind::Rebuild,
                command: Some(String::from("turbo run <task>")),
                detail: String::from("Turborepo local cache is generated by task execution."),
            },
            "package.json",
        ),
        "target" if parent.join("Cargo.toml").is_file() => (
            CandidateKind::RustTarget,
            RecoveryContract {
                kind: RecoveryKind::Rebuild,
                command: Some(String::from("cargo build")),
                detail: String::from(
                    "Cargo build output is reproducible unless provider-specific protected state is present.",
                ),
            },
            "Cargo.toml",
        ),
        ".zig-cache" if parent.join("build.zig").is_file() => (
            CandidateKind::ZigCache,
            RecoveryContract {
                kind: RecoveryKind::Rebuild,
                command: Some(String::from("zig build")),
                detail: String::from("Zig local build cache is generated by the build system."),
            },
            "build.zig",
        ),
        "zig-out" if parent.join("build.zig").is_file() => (
            CandidateKind::ZigOutput,
            RecoveryContract {
                kind: RecoveryKind::Rebuild,
                command: Some(String::from("zig build")),
                detail: String::from("Zig installation output is generated by the project build."),
            },
            "build.zig",
        ),
        ".build" if parent.join("Package.swift").is_file() => (
            CandidateKind::SwiftBuild,
            RecoveryContract {
                kind: RecoveryKind::Rebuild,
                command: Some(String::from("swift build")),
                detail: String::from("Swift Package Manager build state is reproducible."),
            },
            "Package.swift",
        ),
        _ if is_xcode_derived_data_entry(path) => (
            CandidateKind::XcodeDerivedData,
            RecoveryContract {
                kind: RecoveryKind::IdeRegeneration,
                command: None,
                detail: String::from("Xcode DerivedData is regenerated by Xcode/build tooling."),
            },
            "Xcode DerivedData boundary",
        ),
        _ => return None,
    };

    evidence.push(Evidence::proven(
        "provider_boundary",
        format!("Recognized provider marker: {}.", result.2),
    ));

    Some((result.0, result.1))
}

fn node_restore_command(parent: &Path) -> Option<String> {
    if parent.join("pnpm-lock.yaml").is_file() {
        return Some(String::from("pnpm install --frozen-lockfile"));
    }

    if parent.join("yarn.lock").is_file() {
        return Some(String::from("yarn install"));
    }

    if parent.join("package-lock.json").is_file() {
        return Some(String::from("npm ci"));
    }

    None
}

fn contains_anchor_keypair(target: &Path) -> bool {
    let deploy = target.join("deploy");
    let Ok(entries) = fs::read_dir(deploy) else {
        return false;
    };

    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.ends_with("-keypair.json"))
    })
}

fn is_xcode_derived_data_entry(path: &Path) -> bool {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return false;
    };

    let root = home.join("Library/Developer/Xcode/DerivedData");

    path.parent().is_some_and(|parent| parent == root)
}
