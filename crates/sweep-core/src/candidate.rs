use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Evidence, FileIdentity};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    NodeModules,
    NextBuild,
    TurboCache,
    RustTarget,
    ZigCache,
    ZigOutput,
    SwiftBuild,
    XcodeDerivedData,
    GoBuildCache,
    GoModuleCache,
    PytestCache,
    MypyCache,
    RuffCache,
    PythonVirtualEnv,
    Unrecognized,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Safe,
    Review,
    Protected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKind {
    Rebuild,
    ReinstallDependencies,
    IdeRegeneration,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryContract {
    pub kind: RecoveryKind,
    pub command: Option<String>,
    pub detail: String,
}

impl RecoveryContract {
    pub fn none(detail: impl Into<String>) -> Self {
        Self {
            kind: RecoveryKind::None,
            command: None,
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub path: PathBuf,
    pub kind: CandidateKind,
    pub decision: Decision,
    pub logical_bytes: u64,
    pub allocated_bytes_estimate: u64,
    pub traversal_complete: bool,
    pub subtree_metadata_fingerprint: Option<String>,
    pub identity: Option<FileIdentity>,
    pub recovery: RecoveryContract,
    pub evidence: Vec<Evidence>,
}
