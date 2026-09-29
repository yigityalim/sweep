use std::{
    env,
    path::{Path, PathBuf},
};

use sweep_core::{CandidateKind, Decision, Evidence, RecoveryContract};

mod cargo;
mod go;
mod node;
mod python;
mod swift;
mod xcode;
mod zig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitEvidencePolicy {
    Required,
    NotApplicable,
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderContext {
    home: Option<PathBuf>,
}

impl ProviderContext {
    fn from_environment() -> Self {
        Self {
            home: env::var_os("HOME").map(PathBuf::from),
        }
    }

    pub(crate) fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderAssessment {
    pub(crate) kind: CandidateKind,
    pub(crate) decision: Decision,
    pub(crate) recovery: RecoveryContract,
    pub(crate) evidence: Vec<Evidence>,
    pub(crate) git_evidence: GitEvidencePolicy,
}

impl ProviderAssessment {
    fn new(
        kind: CandidateKind,
        recovery: RecoveryContract,
        git_evidence: GitEvidencePolicy,
        marker: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            decision: Decision::Safe,
            recovery,
            evidence: vec![Evidence::proven(
                "provider_boundary",
                format!("Recognized provider marker: {}.", marker.into()),
            )],
            git_evidence,
        }
    }

    fn review(&mut self, code: impl Into<String>, detail: impl Into<String>) {
        self.decision = self.decision.max(Decision::Review);
        self.evidence.push(Evidence::unknown(code, detail));
    }

    fn prove(&mut self, code: impl Into<String>, detail: impl Into<String>) {
        self.evidence.push(Evidence::proven(code, detail));
    }
}

pub(crate) trait Provider: Sync {
    fn assess(&self, path: &Path, context: &ProviderContext) -> Option<ProviderAssessment>;
}

static NODE: node::NodeProvider = node::NodeProvider;
static CARGO: cargo::CargoProvider = cargo::CargoProvider;
static GO: go::GoProvider = go::GoProvider;
static PYTHON: python::PythonProvider = python::PythonProvider;
static ZIG: zig::ZigProvider = zig::ZigProvider;
static SWIFT: swift::SwiftProvider = swift::SwiftProvider;
static XCODE: xcode::XcodeProvider = xcode::XcodeProvider;

static PROVIDERS: [&dyn Provider; 7] = [&NODE, &CARGO, &ZIG, &SWIFT, &PYTHON, &GO, &XCODE];

pub(crate) fn assess(path: &Path) -> Option<ProviderAssessment> {
    let context = ProviderContext::from_environment();
    PROVIDERS
        .iter()
        .find_map(|provider| provider.assess(path, &context))
}

pub(crate) fn candidate_kind(path: &Path) -> Option<CandidateKind> {
    assess(path).map(|assessment| assessment.kind)
}
