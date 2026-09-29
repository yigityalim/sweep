#![forbid(unsafe_code)]

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sweep_core::{Candidate, CandidateKind, Decision, EvidenceStatus, RecoveryKind};

mod snapshot;

pub use snapshot::{
    AddedCandidate, ByteDelta, ChangedCandidate, DeltaDirection, MovedCandidate, RemovedCandidate,
    SNAPSHOT_SCHEMA_VERSION, Snapshot, SnapshotCandidate, SnapshotDiff, SnapshotDiffFormat,
    SnapshotDiffSummary, SnapshotError, SnapshotIdentity, SnapshotSummary, render_snapshot_diff,
};

pub const REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Text,
    Markdown,
    Json,
    Toml,
}

impl OutputFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Markdown => "markdown",
            Self::Json => "json",
            Self::Toml => "toml",
        }
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Markdown => "md",
            Self::Json => "json",
            Self::Toml => "toml",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub created_unix_seconds: u64,
    pub root: String,
    pub summary: ReportSummary,
    pub candidates: Vec<ReportCandidate>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReportSummary {
    pub candidate_count: usize,
    pub safe_count: usize,
    pub review_count: usize,
    pub protected_count: usize,
    pub logical_bytes: u64,
    pub allocated_bytes_estimate: u64,
    pub safe_allocated_bytes_estimate: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReportCandidate {
    pub path: String,
    pub kind: String,
    pub decision: String,
    pub logical_bytes: u64,
    pub allocated_bytes_estimate: u64,
    pub traversal_complete: bool,
    pub subtree_metadata_fingerprint: Option<String>,
    pub recovery: ReportRecovery,
    pub evidence: Vec<ReportEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReportRecovery {
    pub kind: String,
    pub command: Option<String>,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReportEvidence {
    pub code: String,
    pub status: String,
    pub detail: String,
}

impl Report {
    pub fn from_candidates(
        root: &Path,
        candidates: &[Candidate],
        redact_home: Option<&Path>,
    ) -> Self {
        let created_unix_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let candidates: Vec<_> = candidates
            .iter()
            .map(|candidate| ReportCandidate::from_candidate(candidate, redact_home))
            .collect();

        let summary = ReportSummary::from_candidates(&candidates);

        Self {
            schema_version: REPORT_SCHEMA_VERSION,
            created_unix_seconds,
            root: display_path(root, redact_home),
            summary,
            candidates,
        }
    }
}

impl ReportSummary {
    fn from_candidates(candidates: &[ReportCandidate]) -> Self {
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

            match candidate.decision.as_str() {
                "safe" => {
                    safe_count += 1;
                    safe_allocated_bytes_estimate = safe_allocated_bytes_estimate
                        .saturating_add(candidate.allocated_bytes_estimate);
                }
                "review" => review_count += 1,
                "protected" => protected_count += 1,
                _ => {}
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

impl ReportCandidate {
    fn from_candidate(candidate: &Candidate, redact_home: Option<&Path>) -> Self {
        Self {
            path: display_path(&candidate.path, redact_home),
            kind: candidate_kind_name(candidate.kind).to_owned(),
            decision: decision_name(candidate.decision).to_owned(),
            logical_bytes: candidate.logical_bytes,
            allocated_bytes_estimate: candidate.allocated_bytes_estimate,
            traversal_complete: candidate.traversal_complete,
            subtree_metadata_fingerprint: candidate.subtree_metadata_fingerprint.clone(),
            recovery: ReportRecovery {
                kind: recovery_kind_name(&candidate.recovery.kind).to_owned(),
                command: candidate.recovery.command.clone(),
                detail: candidate.recovery.detail.clone(),
            },
            evidence: candidate
                .evidence
                .iter()
                .map(|evidence| ReportEvidence {
                    code: evidence.code.clone(),
                    status: evidence_status_name(&evidence.status).to_owned(),
                    detail: evidence.detail.clone(),
                })
                .collect(),
        }
    }
}

pub fn render(report: &Report, format: OutputFormat) -> Result<String, serde_json::Error> {
    match format {
        OutputFormat::Text => Ok(render_text(report)),
        OutputFormat::Markdown => Ok(render_markdown(report)),
        OutputFormat::Json => serde_json::to_string_pretty(report),
        OutputFormat::Toml => Ok(render_toml(report)),
    }
}

pub const fn candidate_kind_name(kind: CandidateKind) -> &'static str {
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

pub const fn decision_name(decision: Decision) -> &'static str {
    match decision {
        Decision::Safe => "safe",
        Decision::Review => "review",
        Decision::Protected => "protected",
    }
}

fn recovery_kind_name(kind: &RecoveryKind) -> &'static str {
    match kind {
        RecoveryKind::Rebuild => "rebuild",
        RecoveryKind::ReinstallDependencies => "reinstall_dependencies",
        RecoveryKind::IdeRegeneration => "ide_regeneration",
        RecoveryKind::None => "none",
    }
}

fn evidence_status_name(status: &EvidenceStatus) -> &'static str {
    match status {
        EvidenceStatus::Proven => "proven",
        EvidenceStatus::Refuted => "refuted",
        EvidenceStatus::Unknown => "unknown",
    }
}

fn display_path(path: &Path, redact_home: Option<&Path>) -> String {
    if let Some(home) = redact_home
        && let Ok(relative) = path.strip_prefix(home)
    {
        if relative.as_os_str().is_empty() {
            return String::from("~");
        }

        return PathBuf::from("~")
            .join(relative)
            .to_string_lossy()
            .into_owned();
    }

    path.to_string_lossy().into_owned()
}

fn human_visible(candidate: &ReportCandidate) -> bool {
    candidate.allocated_bytes_estimate > 0 || candidate.decision == "protected"
}

fn hidden_human_candidate_count(report: &Report) -> usize {
    report
        .candidates
        .iter()
        .filter(|candidate| !human_visible(candidate))
        .count()
}

fn render_text(report: &Report) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Sweep storage report");
    let _ = writeln!(out, "Root: {}", report.root);
    let _ = writeln!(
        out,
        "Candidates: {} ({} safe, {} review, {} protected)",
        report.summary.candidate_count,
        report.summary.safe_count,
        report.summary.review_count,
        report.summary.protected_count
    );
    let _ = writeln!(
        out,
        "Observed allocated-size estimate: {}",
        format_bytes(report.summary.allocated_bytes_estimate)
    );
    let _ = writeln!(
        out,
        "Safe allocated-size estimate: {}",
        format_bytes(report.summary.safe_allocated_bytes_estimate)
    );

    let hidden = hidden_human_candidate_count(report);
    if hidden > 0 {
        let _ = writeln!(
            out,
            "Detailed view omits {hidden} non-protected candidates with a 0 B allocated-size estimate."
        );
    }

    for candidate in report
        .candidates
        .iter()
        .filter(|candidate| human_visible(candidate))
    {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{}  {}  {}",
            candidate.decision,
            format_bytes(candidate.allocated_bytes_estimate),
            candidate.kind
        );
        let _ = writeln!(out, "  path: {}", candidate.path);
        let _ = writeln!(out, "  recovery: {}", candidate.recovery.detail);
        if let Some(command) = &candidate.recovery.command {
            let _ = writeln!(out, "  command: {command}");
        }
        for evidence in &candidate.evidence {
            let _ = writeln!(
                out,
                "  {} {}: {}",
                evidence.status, evidence.code, evidence.detail
            );
        }
    }

    out
}

fn render_markdown(report: &Report) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Sweep Storage Report");
    let _ = writeln!(out);
    let _ = writeln!(out, "- **Root:** {}", markdown_escape(&report.root));
    let _ = writeln!(out, "- **Candidates:** {}", report.summary.candidate_count);
    let _ = writeln!(out, "- **Safe:** {}", report.summary.safe_count);
    let _ = writeln!(out, "- **Review:** {}", report.summary.review_count);
    let _ = writeln!(out, "- **Protected:** {}", report.summary.protected_count);
    let _ = writeln!(
        out,
        "- **Observed allocated-size estimate:** {}",
        format_bytes(report.summary.allocated_bytes_estimate)
    );
    let _ = writeln!(
        out,
        "- **Safe allocated-size estimate:** {}",
        format_bytes(report.summary.safe_allocated_bytes_estimate)
    );
    let hidden = hidden_human_candidate_count(report);
    if hidden > 0 {
        let _ = writeln!(
            out,
            "- **Detailed view omitted:** {hidden} non-protected candidates with a 0 B allocated-size estimate"
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "| Decision | Allocated estimate | Kind | Path |");
    let _ = writeln!(out, "| --- | ---: | --- | --- |");

    for candidate in report
        .candidates
        .iter()
        .filter(|candidate| human_visible(candidate))
    {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            candidate.decision,
            format_bytes(candidate.allocated_bytes_estimate),
            candidate.kind,
            markdown_escape(&candidate.path)
        );
    }

    for candidate in report
        .candidates
        .iter()
        .filter(|candidate| human_visible(candidate))
    {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "## {} — {}",
            markdown_escape(&candidate.kind),
            markdown_escape(&candidate.path)
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "- **Decision:** {}", candidate.decision);
        let _ = writeln!(
            out,
            "- **Allocated estimate:** {}",
            format_bytes(candidate.allocated_bytes_estimate)
        );
        let _ = writeln!(
            out,
            "- **Logical size:** {}",
            format_bytes(candidate.logical_bytes)
        );
        let _ = writeln!(
            out,
            "- **Recovery:** {}",
            markdown_escape(&candidate.recovery.detail)
        );
        if let Some(command) = &candidate.recovery.command {
            let _ = writeln!(out, "- **Command:** `{}`", markdown_escape(command));
        }

        let _ = writeln!(out);
        let _ = writeln!(out, "### Evidence");
        for evidence in &candidate.evidence {
            let _ = writeln!(
                out,
                "- **{} / {}:** {}",
                evidence.status,
                markdown_escape(&evidence.code),
                markdown_escape(&evidence.detail)
            );
        }
    }

    out
}

fn render_toml(report: &Report) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "schema_version = {}", report.schema_version);
    let _ = writeln!(
        out,
        "created_unix_seconds = {}",
        report.created_unix_seconds
    );
    let _ = writeln!(out, "root = {}", toml_quote(&report.root));
    let _ = writeln!(out);
    let _ = writeln!(out, "[summary]");
    let _ = writeln!(out, "candidate_count = {}", report.summary.candidate_count);
    let _ = writeln!(out, "safe_count = {}", report.summary.safe_count);
    let _ = writeln!(out, "review_count = {}", report.summary.review_count);
    let _ = writeln!(out, "protected_count = {}", report.summary.protected_count);
    let _ = writeln!(out, "logical_bytes = {}", report.summary.logical_bytes);
    let _ = writeln!(
        out,
        "allocated_bytes_estimate = {}",
        report.summary.allocated_bytes_estimate
    );
    let _ = writeln!(
        out,
        "safe_allocated_bytes_estimate = {}",
        report.summary.safe_allocated_bytes_estimate
    );

    for candidate in &report.candidates {
        let _ = writeln!(out);
        let _ = writeln!(out, "[[candidates]]");
        let _ = writeln!(out, "path = {}", toml_quote(&candidate.path));
        let _ = writeln!(out, "kind = {}", toml_quote(&candidate.kind));
        let _ = writeln!(out, "decision = {}", toml_quote(&candidate.decision));
        let _ = writeln!(out, "logical_bytes = {}", candidate.logical_bytes);
        let _ = writeln!(
            out,
            "allocated_bytes_estimate = {}",
            candidate.allocated_bytes_estimate
        );
        let _ = writeln!(out, "traversal_complete = {}", candidate.traversal_complete);
        if let Some(fingerprint) = &candidate.subtree_metadata_fingerprint {
            let _ = writeln!(
                out,
                "subtree_metadata_fingerprint = {}",
                toml_quote(fingerprint)
            );
        }

        let _ = writeln!(out);
        let _ = writeln!(out, "[candidates.recovery]");
        let _ = writeln!(out, "kind = {}", toml_quote(&candidate.recovery.kind));
        if let Some(command) = &candidate.recovery.command {
            let _ = writeln!(out, "command = {}", toml_quote(command));
        }
        let _ = writeln!(out, "detail = {}", toml_quote(&candidate.recovery.detail));

        for evidence in &candidate.evidence {
            let _ = writeln!(out);
            let _ = writeln!(out, "[[candidates.evidence]]");
            let _ = writeln!(out, "code = {}", toml_quote(&evidence.code));
            let _ = writeln!(out, "status = {}", toml_quote(&evidence.status));
            let _ = writeln!(out, "detail = {}", toml_quote(&evidence.detail));
        }
    }

    out
}

fn markdown_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('|', "\\|")
}

fn toml_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');

    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            character if character.is_control() => {
                let _ = write!(out, "\\u{:04X}", character as u32);
            }
            character => out.push(character),
        }
    }

    out.push('"');
    out
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sweep_core::{Evidence, RecoveryContract, RecoveryKind};

    use super::*;

    fn candidate(decision: Decision, path: &str, bytes: u64) -> Candidate {
        Candidate {
            path: PathBuf::from(path),
            kind: CandidateKind::RustTarget,
            decision,
            logical_bytes: bytes,
            allocated_bytes_estimate: bytes,
            traversal_complete: true,
            subtree_metadata_fingerprint: Some(String::from("abc")),
            identity: None,
            recovery: RecoveryContract {
                kind: RecoveryKind::Rebuild,
                command: Some(String::from("cargo build")),
                detail: String::from("rebuildable"),
            },
            evidence: vec![Evidence::proven("test", "proven")],
        }
    }

    #[test]
    fn report_summary_preserves_decision_counts_and_estimates() {
        let report = Report::from_candidates(
            Path::new("/tmp"),
            &[
                candidate(Decision::Safe, "/tmp/a", 10),
                candidate(Decision::Review, "/tmp/b", 20),
                candidate(Decision::Protected, "/tmp/c", 30),
            ],
            None,
        );

        assert_eq!(report.summary.candidate_count, 3);
        assert_eq!(report.summary.safe_count, 1);
        assert_eq!(report.summary.review_count, 1);
        assert_eq!(report.summary.protected_count, 1);
        assert_eq!(report.summary.allocated_bytes_estimate, 60);
        assert_eq!(report.summary.safe_allocated_bytes_estimate, 10);
    }

    #[test]
    fn home_redaction_respects_path_boundaries() {
        let candidates = [
            candidate(Decision::Safe, "/Users/demo/Developer/app/target", 10),
            candidate(Decision::Safe, "/Users/demonstration/target", 20),
        ];

        let report = Report::from_candidates(
            Path::new("/Users/demo"),
            &candidates,
            Some(Path::new("/Users/demo")),
        );

        assert_eq!(report.root, "~");
        assert_eq!(report.candidates[0].path, "~/Developer/app/target");
        assert_eq!(report.candidates[1].path, "/Users/demonstration/target");
    }

    #[test]
    fn all_formats_render_from_the_same_report() {
        let report = Report::from_candidates(
            Path::new("/tmp"),
            &[candidate(Decision::Safe, "/tmp/a", 10)],
            None,
        );

        for format in [
            OutputFormat::Text,
            OutputFormat::Markdown,
            OutputFormat::Json,
            OutputFormat::Toml,
        ] {
            let output = render(&report, format).unwrap();
            assert!(!output.is_empty());
            assert!(output.contains("safe"));
        }
    }

    #[test]
    fn human_reports_hide_zero_allocated_noise_but_keep_protected_entries() {
        let report = Report::from_candidates(
            Path::new("/tmp"),
            &[
                candidate(Decision::Safe, "/tmp/zero-safe", 0),
                candidate(Decision::Protected, "/tmp/zero-protected", 0),
                candidate(Decision::Safe, "/tmp/nonzero", 10),
            ],
            None,
        );

        let text = render(&report, OutputFormat::Text).unwrap();
        assert!(!text.contains("/tmp/zero-safe"));
        assert!(text.contains("/tmp/zero-protected"));
        assert!(text.contains("omits 1 non-protected candidates"));

        let markdown = render(&report, OutputFormat::Markdown).unwrap();
        assert!(!markdown.contains("/tmp/zero-safe"));
        assert!(markdown.contains("/tmp/zero-protected"));

        let json = render(&report, OutputFormat::Json).unwrap();
        assert!(json.contains("/tmp/zero-safe"));

        let toml = render(&report, OutputFormat::Toml).unwrap();
        assert!(toml.contains("/tmp/zero-safe"));
    }

    #[test]
    fn toml_string_escaping_handles_control_characters() {
        assert_eq!(toml_quote("a\"b\nc"), "\"a\\\"b\\nc\"");
    }
}
