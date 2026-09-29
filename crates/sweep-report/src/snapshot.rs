use std::{
    collections::BTreeSet,
    error::Error,
    fmt::{self, Write as _},
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sweep_core::{Candidate, CandidateKind, Decision, RecoveryKind};

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 2;
const MIN_SUPPORTED_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub created_unix_seconds: u64,
    pub root: String,
    pub complete: bool,
    pub discovery_error_count: usize,
    pub summary: SnapshotSummary,
    pub candidates: Vec<SnapshotCandidate>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub candidate_count: usize,
    pub safe_count: usize,
    pub review_count: usize,
    pub protected_count: usize,
    pub logical_bytes: u64,
    pub allocated_bytes_estimate: u64,
    pub safe_allocated_bytes_estimate: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotCandidate {
    pub relative_path: String,
    #[serde(default, skip_serializing)]
    pub path: Option<String>,
    pub kind: CandidateKind,
    pub decision: Decision,
    pub logical_bytes: u64,
    pub allocated_bytes_estimate: u64,
    pub traversal_complete: bool,
    pub subtree_metadata_fingerprint: Option<String>,
    pub identity: Option<SnapshotIdentity>,
    pub recovery_kind: RecoveryKind,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct SnapshotIdentity {
    pub device: u64,
    pub inode: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaDirection {
    Increased,
    Decreased,
    Unchanged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ByteDelta {
    pub direction: DeltaDirection,
    pub bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotDiff {
    pub schema_version: u32,
    pub root: String,
    pub from_created_unix_seconds: u64,
    pub to_created_unix_seconds: u64,
    pub complete: bool,
    pub summary: SnapshotDiffSummary,
    pub added: Vec<AddedCandidate>,
    pub removed: Vec<RemovedCandidate>,
    pub changed: Vec<ChangedCandidate>,
    pub moved: Vec<MovedCandidate>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotDiffSummary {
    pub before_allocated_bytes_estimate: u64,
    pub after_allocated_bytes_estimate: u64,
    pub allocated_bytes_estimate_delta: ByteDelta,
    pub added_count: usize,
    pub removed_count: usize,
    pub changed_count: usize,
    pub moved_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AddedCandidate {
    pub relative_path: String,
    pub kind: CandidateKind,
    pub decision: Decision,
    pub allocated_bytes_estimate: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RemovedCandidate {
    pub relative_path: String,
    pub kind: CandidateKind,
    pub decision: Decision,
    pub allocated_bytes_estimate: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangedCandidate {
    pub relative_path: String,
    pub kind: CandidateKind,
    pub before_decision: Decision,
    pub after_decision: Decision,
    pub before_allocated_bytes_estimate: u64,
    pub after_allocated_bytes_estimate: u64,
    pub allocated_bytes_estimate_delta: ByteDelta,
    pub fingerprint_changed: bool,
    pub traversal_complete_changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MovedCandidate {
    pub from_relative_path: String,
    pub to_relative_path: String,
    pub kind: CandidateKind,
    pub before_decision: Decision,
    pub after_decision: Decision,
    pub before_allocated_bytes_estimate: u64,
    pub after_allocated_bytes_estimate: u64,
    pub allocated_bytes_estimate_delta: ByteDelta,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotDiffFormat {
    Text,
    Markdown,
    Json,
}

#[derive(Debug, Eq, PartialEq)]
pub enum SnapshotError {
    UnsupportedSchemaVersion(u32),
    InvalidRelativePath(String),
    DuplicateCandidate(String),
    SummaryMismatch,
    InvalidCompleteness,
    RootMismatch { before: String, after: String },
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "unsupported snapshot schema version {version}")
            }
            Self::InvalidRelativePath(path) => {
                write!(formatter, "snapshot contains an invalid relative path: {path:?}")
            }
            Self::DuplicateCandidate(key) => {
                write!(
                    formatter,
                    "snapshot contains a duplicate candidate key: {key:?}"
                )
            }
            Self::SummaryMismatch => {
                write!(
                    formatter,
                    "snapshot summary does not match its candidate set"
                )
            }
            Self::InvalidCompleteness => {
                write!(
                    formatter,
                    "snapshot is marked complete despite discovery or candidate traversal errors"
                )
            }
            Self::RootMismatch { before, after } => {
                write!(
                    formatter,
                    "snapshot roots differ: {before:?} versus {after:?}"
                )
            }
        }
    }
}

impl Error for SnapshotError {}

impl Snapshot {
    pub fn from_candidates(
        root: &Path,
        candidates: &[Candidate],
        discovery_complete: bool,
        discovery_error_count: usize,
    ) -> Self {
        let created_unix_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let source_candidate_count = candidates.len();
        let candidates: Vec<_> = candidates
            .iter()
            .filter_map(|candidate| SnapshotCandidate::from_candidate(root, candidate))
            .collect();

        let complete = discovery_complete
            && candidates.len() == source_candidate_count
            && candidates
                .iter()
                .all(|candidate| candidate.traversal_complete);
        let summary = SnapshotSummary::from_candidates(&candidates);

        Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            created_unix_seconds,
            root: root.to_string_lossy().into_owned(),
            complete,
            discovery_error_count,
            summary,
            candidates,
        }
    }

    pub fn validate(&self) -> Result<(), SnapshotError> {
        if !(MIN_SUPPORTED_SNAPSHOT_SCHEMA_VERSION..=SNAPSHOT_SCHEMA_VERSION)
            .contains(&self.schema_version)
        {
            return Err(SnapshotError::UnsupportedSchemaVersion(self.schema_version));
        }

        let mut keys = BTreeSet::new();
        for candidate in &self.candidates {
            validate_relative_path(&candidate.relative_path)?;
            let key = candidate.exact_key();
            if !keys.insert(key.clone()) {
                return Err(SnapshotError::DuplicateCandidate(key));
            }
        }

        if SnapshotSummary::from_candidates(&self.candidates) != self.summary {
            return Err(SnapshotError::SummaryMismatch);
        }

        if self.complete
            && (self.discovery_error_count != 0
                || self
                    .candidates
                    .iter()
                    .any(|candidate| !candidate.traversal_complete))
        {
            return Err(SnapshotError::InvalidCompleteness);
        }

        Ok(())
    }

    pub fn diff(&self, after: &Self) -> Result<SnapshotDiff, SnapshotError> {
        self.validate()?;
        after.validate()?;

        if self.root != after.root {
            return Err(SnapshotError::RootMismatch {
                before: self.root.clone(),
                after: after.root.clone(),
            });
        }

        Ok(diff_snapshots(self, after))
    }
}

impl SnapshotSummary {
    fn from_candidates(candidates: &[SnapshotCandidate]) -> Self {
        let mut safe_count = 0;
        let mut review_count = 0;
        let mut protected_count = 0;
        let mut logical_bytes = 0_u64;
        let mut allocated_bytes_estimate = 0_u64;
        let mut safe_allocated_bytes_estimate = 0_u64;

        for candidate in candidates {
            logical_bytes = logical_bytes.saturating_add(candidate.logical_bytes);
            allocated_bytes_estimate =
                allocated_bytes_estimate.saturating_add(candidate.allocated_bytes_estimate);

            match candidate.decision {
                Decision::Safe => {
                    safe_count += 1;
                    safe_allocated_bytes_estimate = safe_allocated_bytes_estimate
                        .saturating_add(candidate.allocated_bytes_estimate);
                }
                Decision::Review => review_count += 1,
                Decision::Protected => protected_count += 1,
            }
        }

        Self {
            candidate_count: candidates.len(),
            safe_count,
            review_count,
            protected_count,
            logical_bytes,
            allocated_bytes_estimate,
            safe_allocated_bytes_estimate,
        }
    }
}

impl SnapshotCandidate {
    fn from_candidate(root: &Path, candidate: &Candidate) -> Option<Self> {
        let relative = candidate.path.strip_prefix(root).ok()?;
        let relative_path = if relative.as_os_str().is_empty() {
            String::from(".")
        } else {
            relative.to_string_lossy().into_owned()
        };

        Some(Self {
            relative_path,
            path: None,
            kind: candidate.kind,
            decision: candidate.decision,
            logical_bytes: candidate.logical_bytes,
            allocated_bytes_estimate: candidate.allocated_bytes_estimate,
            traversal_complete: candidate.traversal_complete,
            subtree_metadata_fingerprint: candidate.subtree_metadata_fingerprint.clone(),
            identity: candidate
                .identity
                .as_ref()
                .map(|identity| SnapshotIdentity {
                    device: identity.device,
                    inode: identity.inode,
                }),
            recovery_kind: candidate.recovery.kind.clone(),
        })
    }

    fn exact_key(&self) -> String {
        format!(
            "{}\u{0}{}",
            candidate_kind_name(self.kind),
            self.relative_path
        )
    }

    fn materially_differs(&self, other: &Self) -> bool {
        self.decision != other.decision
            || self.logical_bytes != other.logical_bytes
            || self.allocated_bytes_estimate != other.allocated_bytes_estimate
            || self.traversal_complete != other.traversal_complete
            || self.subtree_metadata_fingerprint != other.subtree_metadata_fingerprint
            || self.recovery_kind != other.recovery_kind
    }
}

fn validate_relative_path(value: &str) -> Result<(), SnapshotError> {
    if value.is_empty() {
        return Err(SnapshotError::InvalidRelativePath(value.to_owned()));
    }

    let path = Path::new(value);
    let valid = !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));

    if valid {
        Ok(())
    } else {
        Err(SnapshotError::InvalidRelativePath(value.to_owned()))
    }
}

impl ByteDelta {
    pub const fn between(before: u64, after: u64) -> Self {
        if after > before {
            Self {
                direction: DeltaDirection::Increased,
                bytes: after - before,
            }
        } else if before > after {
            Self {
                direction: DeltaDirection::Decreased,
                bytes: before - after,
            }
        } else {
            Self {
                direction: DeltaDirection::Unchanged,
                bytes: 0,
            }
        }
    }
}

pub fn render_snapshot_diff(
    diff: &SnapshotDiff,
    format: SnapshotDiffFormat,
) -> Result<String, serde_json::Error> {
    match format {
        SnapshotDiffFormat::Text => Ok(render_diff_text(diff)),
        SnapshotDiffFormat::Markdown => Ok(render_diff_markdown(diff)),
        SnapshotDiffFormat::Json => serde_json::to_string_pretty(diff),
    }
}

fn diff_snapshots(before: &Snapshot, after: &Snapshot) -> SnapshotDiff {
    let mut before_matched = vec![false; before.candidates.len()];
    let mut after_matched = vec![false; after.candidates.len()];
    let mut changed = Vec::new();
    let mut moved = Vec::new();

    for (before_index, before_candidate) in before.candidates.iter().enumerate() {
        let before_key = before_candidate.exact_key();
        let Some((after_index, after_candidate)) =
            after
                .candidates
                .iter()
                .enumerate()
                .find(|(index, candidate)| {
                    !after_matched[*index] && candidate.exact_key() == before_key
                })
        else {
            continue;
        };

        before_matched[before_index] = true;
        after_matched[after_index] = true;

        if before_candidate.materially_differs(after_candidate) {
            changed.push(ChangedCandidate {
                relative_path: before_candidate.relative_path.clone(),
                kind: before_candidate.kind,
                before_decision: before_candidate.decision,
                after_decision: after_candidate.decision,
                before_allocated_bytes_estimate: before_candidate.allocated_bytes_estimate,
                after_allocated_bytes_estimate: after_candidate.allocated_bytes_estimate,
                allocated_bytes_estimate_delta: ByteDelta::between(
                    before_candidate.allocated_bytes_estimate,
                    after_candidate.allocated_bytes_estimate,
                ),
                fingerprint_changed: before_candidate.subtree_metadata_fingerprint
                    != after_candidate.subtree_metadata_fingerprint,
                traversal_complete_changed: before_candidate.traversal_complete
                    != after_candidate.traversal_complete,
            });
        }
    }

    for (before_index, before_candidate) in before.candidates.iter().enumerate() {
        if before_matched[before_index] {
            continue;
        }

        let Some(identity) = before_candidate.identity else {
            continue;
        };

        let mut matching_after =
            after
                .candidates
                .iter()
                .enumerate()
                .filter(|(index, candidate)| {
                    !after_matched[*index]
                        && candidate.kind == before_candidate.kind
                        && candidate.identity == Some(identity)
                        && before_candidate.subtree_metadata_fingerprint.is_some()
                        && candidate.subtree_metadata_fingerprint
                            == before_candidate.subtree_metadata_fingerprint
                });

        let Some((after_index, after_candidate)) = matching_after.next() else {
            continue;
        };
        if matching_after.next().is_some() {
            continue;
        }
        before_matched[before_index] = true;
        after_matched[after_index] = true;

        moved.push(MovedCandidate {
            from_relative_path: before_candidate.relative_path.clone(),
            to_relative_path: after_candidate.relative_path.clone(),
            kind: before_candidate.kind,
            before_decision: before_candidate.decision,
            after_decision: after_candidate.decision,
            before_allocated_bytes_estimate: before_candidate.allocated_bytes_estimate,
            after_allocated_bytes_estimate: after_candidate.allocated_bytes_estimate,
            allocated_bytes_estimate_delta: ByteDelta::between(
                before_candidate.allocated_bytes_estimate,
                after_candidate.allocated_bytes_estimate,
            ),
        });
    }

    let mut added: Vec<_> = after
        .candidates
        .iter()
        .enumerate()
        .filter(|(index, _)| !after_matched[*index])
        .map(|(_, candidate)| AddedCandidate {
            relative_path: candidate.relative_path.clone(),
            kind: candidate.kind,
            decision: candidate.decision,
            allocated_bytes_estimate: candidate.allocated_bytes_estimate,
        })
        .collect();

    let mut removed: Vec<_> = before
        .candidates
        .iter()
        .enumerate()
        .filter(|(index, _)| !before_matched[*index])
        .map(|(_, candidate)| RemovedCandidate {
            relative_path: candidate.relative_path.clone(),
            kind: candidate.kind,
            decision: candidate.decision,
            allocated_bytes_estimate: candidate.allocated_bytes_estimate,
        })
        .collect();

    added.sort_by(|left, right| {
        right
            .allocated_bytes_estimate
            .cmp(&left.allocated_bytes_estimate)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    removed.sort_by(|left, right| {
        right
            .allocated_bytes_estimate
            .cmp(&left.allocated_bytes_estimate)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    changed.sort_by(|left, right| {
        right
            .allocated_bytes_estimate_delta
            .bytes
            .cmp(&left.allocated_bytes_estimate_delta.bytes)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    moved.sort_by(|left, right| left.from_relative_path.cmp(&right.from_relative_path));

    SnapshotDiff {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        root: before.root.clone(),
        from_created_unix_seconds: before.created_unix_seconds,
        to_created_unix_seconds: after.created_unix_seconds,
        complete: before.complete && after.complete,
        summary: SnapshotDiffSummary {
            before_allocated_bytes_estimate: before.summary.allocated_bytes_estimate,
            after_allocated_bytes_estimate: after.summary.allocated_bytes_estimate,
            allocated_bytes_estimate_delta: ByteDelta::between(
                before.summary.allocated_bytes_estimate,
                after.summary.allocated_bytes_estimate,
            ),
            added_count: added.len(),
            removed_count: removed.len(),
            changed_count: changed.len(),
            moved_count: moved.len(),
        },
        added,
        removed,
        changed,
        moved,
    }
}

fn render_diff_text(diff: &SnapshotDiff) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Sweep snapshot diff");
    let _ = writeln!(out, "Root: {}", diff.root);
    let _ = writeln!(
        out,
        "Allocated-size estimate: {} -> {} ({})",
        format_bytes(diff.summary.before_allocated_bytes_estimate),
        format_bytes(diff.summary.after_allocated_bytes_estimate),
        format_delta(diff.summary.allocated_bytes_estimate_delta)
    );
    let _ = writeln!(
        out,
        "Changes: {} added, {} removed, {} changed, {} moved",
        diff.summary.added_count,
        diff.summary.removed_count,
        diff.summary.changed_count,
        diff.summary.moved_count
    );
    if !diff.complete {
        let _ = writeln!(
            out,
            "Warning: at least one snapshot is incomplete; this diff may omit filesystem state."
        );
    }

    let growth: Vec<_> = diff
        .changed
        .iter()
        .filter(|candidate| {
            candidate.allocated_bytes_estimate_delta.direction == DeltaDirection::Increased
        })
        .collect();
    if !growth.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "GROWTH");
        for candidate in growth {
            let _ = writeln!(
                out,
                "  {:>12}  {:<16}  {}",
                format_delta(candidate.allocated_bytes_estimate_delta),
                candidate_kind_name(candidate.kind),
                candidate.relative_path
            );
        }
    }

    let shrink: Vec<_> = diff
        .changed
        .iter()
        .filter(|candidate| {
            candidate.allocated_bytes_estimate_delta.direction == DeltaDirection::Decreased
        })
        .collect();
    if !shrink.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "SHRINK");
        for candidate in shrink {
            let _ = writeln!(
                out,
                "  {:>12}  {:<16}  {}",
                format_delta(candidate.allocated_bytes_estimate_delta),
                candidate_kind_name(candidate.kind),
                candidate.relative_path
            );
        }
    }

    if !diff.added.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "NEW");
        for candidate in &diff.added {
            let _ = writeln!(
                out,
                "  {:>12}  {:<16}  {}",
                format!("+{}", format_bytes(candidate.allocated_bytes_estimate)),
                candidate_kind_name(candidate.kind),
                candidate.relative_path
            );
        }
    }

    if !diff.removed.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "REMOVED");
        for candidate in &diff.removed {
            let _ = writeln!(
                out,
                "  {:>12}  {:<16}  {}",
                format!("-{}", format_bytes(candidate.allocated_bytes_estimate)),
                candidate_kind_name(candidate.kind),
                candidate.relative_path
            );
        }
    }

    if !diff.moved.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "MOVED");
        for candidate in &diff.moved {
            let _ = writeln!(
                out,
                "  {:<16}  {} -> {}",
                candidate_kind_name(candidate.kind),
                candidate.from_relative_path,
                candidate.to_relative_path
            );
        }
    }

    let decision_changes: Vec<_> = diff
        .changed
        .iter()
        .filter(|candidate| candidate.before_decision != candidate.after_decision)
        .collect();
    if !decision_changes.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "DECISION CHANGES");
        for candidate in decision_changes {
            let _ = writeln!(
                out,
                "  {} -> {}  {:<16}  {}",
                decision_name(candidate.before_decision),
                decision_name(candidate.after_decision),
                candidate_kind_name(candidate.kind),
                candidate.relative_path
            );
        }
    }

    out
}

fn render_diff_markdown(diff: &SnapshotDiff) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Sweep Snapshot Diff");
    let _ = writeln!(out);
    let _ = writeln!(out, "- **Root:** {}", markdown_escape(&diff.root));
    let _ = writeln!(
        out,
        "- **Allocated-size estimate:** {} → {} ({})",
        format_bytes(diff.summary.before_allocated_bytes_estimate),
        format_bytes(diff.summary.after_allocated_bytes_estimate),
        format_delta(diff.summary.allocated_bytes_estimate_delta)
    );
    let _ = writeln!(
        out,
        "- **Changes:** {} added, {} removed, {} changed, {} moved",
        diff.summary.added_count,
        diff.summary.removed_count,
        diff.summary.changed_count,
        diff.summary.moved_count
    );
    let _ = writeln!(out, "- **Complete:** {}", diff.complete);

    if !diff.changed.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Changed");
        let _ = writeln!(out);
        let _ = writeln!(out, "| Delta | Kind | Decision | Path |");
        let _ = writeln!(out, "| ---: | --- | --- | --- |");
        for candidate in &diff.changed {
            let decision = if candidate.before_decision == candidate.after_decision {
                decision_name(candidate.after_decision).to_owned()
            } else {
                format!(
                    "{} → {}",
                    decision_name(candidate.before_decision),
                    decision_name(candidate.after_decision)
                )
            };
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} |",
                format_delta(candidate.allocated_bytes_estimate_delta),
                candidate_kind_name(candidate.kind),
                decision,
                markdown_escape(&candidate.relative_path)
            );
        }
    }

    if !diff.added.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Added");
        for candidate in &diff.added {
            let _ = writeln!(
                out,
                "- **+{}** {} — {}",
                format_bytes(candidate.allocated_bytes_estimate),
                candidate_kind_name(candidate.kind),
                markdown_escape(&candidate.relative_path)
            );
        }
    }

    if !diff.removed.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Removed");
        for candidate in &diff.removed {
            let _ = writeln!(
                out,
                "- **-{}** {} — {}",
                format_bytes(candidate.allocated_bytes_estimate),
                candidate_kind_name(candidate.kind),
                markdown_escape(&candidate.relative_path)
            );
        }
    }

    if !diff.moved.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Moved");
        for candidate in &diff.moved {
            let _ = writeln!(
                out,
                "- {} — {} → {}",
                candidate_kind_name(candidate.kind),
                markdown_escape(&candidate.from_relative_path),
                markdown_escape(&candidate.to_relative_path)
            );
        }
    }

    out
}

fn candidate_kind_name(kind: CandidateKind) -> &'static str {
    match kind {
        CandidateKind::NodeModules => "node_modules",
        CandidateKind::NextBuild => "next",
        CandidateKind::TurboCache => "turbo",
        CandidateKind::RustTarget => "cargo-target",
        CandidateKind::ZigCache => "zig-cache",
        CandidateKind::ZigOutput => "zig-out",
        CandidateKind::SwiftBuild => "swift-build",
        CandidateKind::XcodeDerivedData => "xcode-derived",
        CandidateKind::GoBuildCache => "go-build-cache",
        CandidateKind::GoModuleCache => "go-mod-cache",
        CandidateKind::PytestCache => "pytest-cache",
        CandidateKind::MypyCache => "mypy-cache",
        CandidateKind::RuffCache => "ruff-cache",
        CandidateKind::PythonVirtualEnv => "python-venv",
        CandidateKind::Unrecognized => "unrecognized",
    }
}

fn decision_name(decision: Decision) -> &'static str {
    match decision {
        Decision::Safe => "safe",
        Decision::Review => "review",
        Decision::Protected => "protected",
    }
}

fn format_delta(delta: ByteDelta) -> String {
    match delta.direction {
        DeltaDirection::Increased => format!("+{}", format_bytes(delta.bytes)),
        DeltaDirection::Decreased => format!("-{}", format_bytes(delta.bytes)),
        DeltaDirection::Unchanged => String::from("0 B"),
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn markdown_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sweep_core::{FileIdentity, RecoveryContract};

    use super::*;

    fn candidate(path: &str, decision: Decision, bytes: u64, inode: u64) -> Candidate {
        Candidate {
            path: PathBuf::from(path),
            kind: CandidateKind::RustTarget,
            decision,
            logical_bytes: bytes,
            allocated_bytes_estimate: bytes,
            traversal_complete: true,
            subtree_metadata_fingerprint: Some(format!("fingerprint-{bytes}")),
            identity: Some(FileIdentity {
                device: 1,
                inode,
                size: 0,
                modified_seconds: 0,
                modified_nanoseconds: 0,
            }),
            recovery: RecoveryContract {
                kind: RecoveryKind::Rebuild,
                command: Some(String::from("cargo build")),
                detail: String::from("rebuildable"),
            },
            evidence: Vec::new(),
        }
    }

    fn snapshot(candidates: &[Candidate]) -> Snapshot {
        Snapshot::from_candidates(Path::new("/workspace"), candidates, true, 0)
    }

    #[test]
    fn summary_preserves_decisions_and_estimates() {
        let snapshot = snapshot(&[
            candidate("/workspace/a", Decision::Safe, 10, 1),
            candidate("/workspace/b", Decision::Review, 20, 2),
            candidate("/workspace/c", Decision::Protected, 30, 3),
        ]);

        assert_eq!(snapshot.summary.candidate_count, 3);
        assert_eq!(snapshot.summary.safe_count, 1);
        assert_eq!(snapshot.summary.review_count, 1);
        assert_eq!(snapshot.summary.protected_count, 1);
        assert_eq!(snapshot.summary.allocated_bytes_estimate, 60);
        assert_eq!(snapshot.summary.safe_allocated_bytes_estimate, 10);
        assert!(snapshot.complete);
    }

    #[test]
    fn new_snapshot_serialization_omits_absolute_candidate_paths() {
        let snapshot = snapshot(&[candidate("/workspace/a", Decision::Safe, 10, 1)]);
        let json = serde_json::to_value(&snapshot).unwrap();

        assert_eq!(snapshot.schema_version, SNAPSHOT_SCHEMA_VERSION);
        assert_eq!(snapshot.candidates[0].relative_path, "a");
        assert_eq!(snapshot.candidates[0].path, None);
        assert!(json["candidates"][0].get("path").is_none());
    }

    #[test]
    fn legacy_schema_one_snapshot_with_absolute_path_still_validates() {
        let snapshot = snapshot(&[candidate("/workspace/a", Decision::Safe, 10, 1)]);
        let mut json = serde_json::to_value(&snapshot).unwrap();
        json["schema_version"] = serde_json::json!(1);
        json["candidates"][0]["path"] = serde_json::json!("/workspace/a");

        let decoded: Snapshot = serde_json::from_value(json).unwrap();
        let reserialized = serde_json::to_value(&decoded).unwrap();

        assert_eq!(decoded.schema_version, 1);
        assert_eq!(decoded.candidates[0].path.as_deref(), Some("/workspace/a"));
        assert_eq!(decoded.validate(), Ok(()));
        assert!(reserialized["candidates"][0].get("path").is_none());
    }

    #[test]
    fn candidate_outside_snapshot_root_is_omitted_and_marks_snapshot_incomplete() {
        let snapshot = Snapshot::from_candidates(
            Path::new("/workspace"),
            &[
                candidate("/workspace/a", Decision::Safe, 10, 1),
                candidate("/outside/b", Decision::Safe, 20, 2),
            ],
            true,
            0,
        );

        assert_eq!(snapshot.candidates.len(), 1);
        assert_eq!(snapshot.candidates[0].relative_path, "a");
        assert!(!snapshot.complete);
    }

    #[test]
    fn imported_snapshot_rejects_empty_relative_path() {
        let mut snapshot = snapshot(&[candidate("/workspace/a", Decision::Safe, 10, 1)]);
        snapshot.candidates[0].relative_path.clear();

        assert_eq!(
            snapshot.validate(),
            Err(SnapshotError::InvalidRelativePath(String::new()))
        );
    }

    #[test]
    fn imported_snapshot_rejects_absolute_relative_path() {
        let mut snapshot = snapshot(&[candidate("/workspace/a", Decision::Safe, 10, 1)]);
        snapshot.candidates[0].relative_path = String::from("/workspace/a");

        assert_eq!(
            snapshot.validate(),
            Err(SnapshotError::InvalidRelativePath(String::from(
                "/workspace/a"
            )))
        );
    }

    #[test]
    fn imported_snapshot_rejects_parent_traversal() {
        let mut snapshot = snapshot(&[candidate("/workspace/a", Decision::Safe, 10, 1)]);
        snapshot.candidates[0].relative_path = String::from("../outside");

        assert_eq!(
            snapshot.validate(),
            Err(SnapshotError::InvalidRelativePath(String::from("../outside")))
        );
    }

    #[test]
    fn unsupported_snapshot_schema_is_rejected() {
        let mut snapshot = snapshot(&[candidate("/workspace/a", Decision::Safe, 10, 1)]);
        snapshot.schema_version = SNAPSHOT_SCHEMA_VERSION + 1;

        assert_eq!(
            snapshot.validate(),
            Err(SnapshotError::UnsupportedSchemaVersion(
                SNAPSHOT_SCHEMA_VERSION + 1
            ))
        );
    }

    #[test]
    fn diff_tracks_growth_and_additions() {
        let before = snapshot(&[candidate("/workspace/a", Decision::Safe, 10, 1)]);
        let after = snapshot(&[
            candidate("/workspace/a", Decision::Safe, 25, 1),
            candidate("/workspace/b", Decision::Review, 5, 2),
        ]);

        let diff = before.diff(&after).unwrap();

        assert_eq!(diff.summary.added_count, 1);
        assert_eq!(diff.summary.changed_count, 1);
        assert_eq!(
            diff.summary.allocated_bytes_estimate_delta,
            ByteDelta {
                direction: DeltaDirection::Increased,
                bytes: 20,
            }
        );
        assert_eq!(diff.changed[0].allocated_bytes_estimate_delta.bytes, 15);
    }

    #[test]
    fn diff_detects_move_by_filesystem_identity() {
        let before = snapshot(&[candidate("/workspace/old/target", Decision::Safe, 10, 7)]);
        let after = snapshot(&[candidate("/workspace/new/target", Decision::Safe, 10, 7)]);

        let diff = before.diff(&after).unwrap();

        assert_eq!(diff.summary.moved_count, 1);
        assert_eq!(diff.summary.added_count, 0);
        assert_eq!(diff.summary.removed_count, 0);
        assert_eq!(diff.moved[0].from_relative_path, "old/target");
        assert_eq!(diff.moved[0].to_relative_path, "new/target");
    }

    #[test]
    fn root_mismatch_is_rejected() {
        let before = Snapshot::from_candidates(
            Path::new("/one"),
            &[candidate("/one/a", Decision::Safe, 10, 1)],
            true,
            0,
        );
        let after = Snapshot::from_candidates(
            Path::new("/two"),
            &[candidate("/two/a", Decision::Safe, 10, 1)],
            true,
            0,
        );

        assert!(matches!(
            before.diff(&after),
            Err(SnapshotError::RootMismatch { .. })
        ));
    }

    #[test]
    fn incomplete_discovery_propagates_to_diff() {
        let mut before = snapshot(&[candidate("/workspace/a", Decision::Safe, 10, 1)]);
        before.complete = false;
        before.discovery_error_count = 1;
        let after = snapshot(&[candidate("/workspace/a", Decision::Safe, 10, 1)]);

        let diff = before.diff(&after).unwrap();

        assert!(!diff.complete);
        assert!(render_diff_text(&diff).contains("incomplete"));
    }

    #[test]
    fn identity_matching_is_not_used_when_ambiguous() {
        let before = snapshot(&[candidate("/workspace/old", Decision::Safe, 10, 7)]);
        let after = snapshot(&[
            candidate("/workspace/new-a", Decision::Safe, 10, 7),
            candidate("/workspace/new-b", Decision::Safe, 10, 7),
        ]);

        let diff = before.diff(&after).unwrap();

        assert_eq!(diff.summary.moved_count, 0);
        assert_eq!(diff.summary.removed_count, 1);
        assert_eq!(diff.summary.added_count, 2);
    }

    #[test]
    fn exact_path_match_wins_over_identity() {
        let before = snapshot(&[
            candidate("/workspace/a", Decision::Safe, 10, 1),
            candidate("/workspace/b", Decision::Safe, 20, 2),
        ]);
        let after = snapshot(&[
            candidate("/workspace/a", Decision::Safe, 30, 2),
            candidate("/workspace/b", Decision::Safe, 20, 1),
        ]);

        let diff = before.diff(&after).unwrap();

        assert_eq!(diff.summary.moved_count, 0);
        assert_eq!(diff.summary.changed_count, 1);
        assert_eq!(diff.changed[0].relative_path, "a");
    }

    #[test]
    fn changed_candidates_are_sorted_by_absolute_delta() {
        let before = snapshot(&[
            candidate("/workspace/a", Decision::Safe, 100, 1),
            candidate("/workspace/b", Decision::Safe, 100, 2),
        ]);
        let after = snapshot(&[
            candidate("/workspace/a", Decision::Safe, 110, 1),
            candidate("/workspace/b", Decision::Safe, 150, 2),
        ]);

        let diff = before.diff(&after).unwrap();

        assert_eq!(diff.changed[0].relative_path, "b");
        assert_eq!(diff.changed[1].relative_path, "a");
    }

    #[test]
    fn invalid_snapshot_summary_is_rejected() {
        let mut snapshot = snapshot(&[candidate("/workspace/a", Decision::Safe, 10, 1)]);
        snapshot.summary.candidate_count = 2;

        assert_eq!(snapshot.validate(), Err(SnapshotError::SummaryMismatch));
    }

    #[test]
    fn move_detection_requires_matching_fingerprint() {
        let before = snapshot(&[candidate("/workspace/old", Decision::Safe, 10, 7)]);
        let after = snapshot(&[candidate("/workspace/new", Decision::Safe, 20, 7)]);

        let diff = before.diff(&after).unwrap();

        assert_eq!(diff.summary.moved_count, 0);
        assert_eq!(diff.summary.removed_count, 1);
        assert_eq!(diff.summary.added_count, 1);
    }

    #[test]
    fn snapshot_candidates_have_unique_exact_keys() {
        let snapshot = snapshot(&[
            candidate("/workspace/a", Decision::Safe, 10, 1),
            candidate("/workspace/b", Decision::Safe, 20, 2),
        ]);

        let keys: BTreeSet<_> = snapshot
            .candidates
            .iter()
            .map(SnapshotCandidate::exact_key)
            .collect();

        assert_eq!(keys.len(), snapshot.candidates.len());
    }
}
