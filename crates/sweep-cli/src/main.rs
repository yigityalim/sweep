#![forbid(unsafe_code)]

use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use sweep_core::{Candidate, Decision, Plan};
use sweep_report::{
    OutputFormat, Report, Snapshot, SnapshotDiffFormat, render, render_snapshot_diff,
};
use sweep_scan::{ScanOptions, classify_path, scan, scan_with_diagnostics};

#[derive(Parser, Debug)]
#[command(
    name = "sw",
    version,
    about = "Proof-driven disk reclamation for developer Macs",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ReportFormatArg {
    Text,
    Markdown,
    Json,
    Toml,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum DiffFormatArg {
    Text,
    Markdown,
    Json,
}

impl From<DiffFormatArg> for SnapshotDiffFormat {
    fn from(value: DiffFormatArg) -> Self {
        match value {
            DiffFormatArg::Text => Self::Text,
            DiffFormatArg::Markdown => Self::Markdown,
            DiffFormatArg::Json => Self::Json,
        }
    }
}

impl From<ReportFormatArg> for OutputFormat {
    fn from(value: ReportFormatArg) -> Self {
        match value {
            ReportFormatArg::Text => Self::Text,
            ReportFormatArg::Markdown => Self::Markdown,
            ReportFormatArg::Json => Self::Json,
            ReportFormatArg::Toml => Self::Toml,
        }
    }
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Discover and classify supported developer-generated data.
    Scan {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Explain why one path is safe, review-only, or protected.
    Explain {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Render a shareable, versioned storage report.
    Report {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = ReportFormatArg::Text)]
        format: ReportFormatArg,
        #[arg(long, conflicts_with = "save")]
        output: Option<PathBuf>,
        #[arg(long, conflicts_with = "output")]
        save: bool,
        #[arg(long)]
        copy: bool,
        #[arg(long)]
        redact_home: bool,
    },
    /// Persist a read-only storage snapshot for later comparison.
    Snapshot {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Compare two read-only storage snapshots.
    Diff {
        before: PathBuf,
        after: PathBuf,
        #[arg(long, value_enum, default_value_t = DiffFormatArg::Text)]
        format: DiffFormatArg,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Generate a non-destructive immutable plan containing safe candidates only.
    Plan {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Inspect runtime prerequisites and platform assumptions.
    Doctor {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Serialize)]
struct ScanOutput<'a> {
    root: &'a Path,
    candidates: &'a [Candidate],
    allocated_bytes_estimate: u64,
    logical_bytes: u64,
}

#[derive(Serialize)]
struct DoctorOutput {
    os: String,
    architecture: String,
    git: Probe,
    root_filesystem: Probe,
    mutation_enabled: bool,
}

#[derive(Serialize)]
struct Probe {
    status: String,
    detail: String,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sw: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        None => run_tui(),
        Some(Commands::Scan { path, json }) => run_scan(path, json),
        Some(Commands::Explain { path, json }) => run_explain(path, json),
        Some(Commands::Report {
            path,
            format,
            output,
            save,
            copy,
            redact_home,
        }) => run_report(path, format.into(), output, save, copy, redact_home),
        Some(Commands::Snapshot { path, output }) => run_snapshot(path, output),
        Some(Commands::Diff {
            before,
            after,
            format,
            output,
        }) => run_diff(before, after, format.into(), output),
        Some(Commands::Plan { path, output }) => run_plan(path, output),
        Some(Commands::Doctor { json }) => run_doctor(json),
    }
}

fn run_tui() -> Result<(), Box<dyn std::error::Error>> {
    let root = sweep_tui::default_root()?;
    sweep_tui::run(root)?;
    Ok(())
}

fn run_scan(path: PathBuf, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let root = path.canonicalize()?;
    let candidates = scan(&root, &ScanOptions::default())?;

    if json {
        let output = ScanOutput {
            root: &root,
            allocated_bytes_estimate: candidates
                .iter()
                .map(|candidate| candidate.allocated_bytes_estimate)
                .sum(),
            logical_bytes: candidates
                .iter()
                .map(|candidate| candidate.logical_bytes)
                .sum(),
            candidates: &candidates,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    print_scan(&root, &candidates);
    Ok(())
}

fn run_explain(path: PathBuf, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let path = lexical_absolute(path)?;
    fs::symlink_metadata(&path)?;
    let candidate = classify_path(&path);

    if json {
        println!("{}", serde_json::to_string_pretty(&candidate)?);
        return Ok(());
    }

    println!("{}", candidate.path.display());
    println!("  kind:      {}", kind_name(&candidate));
    println!("  decision:  {}", decision_name(candidate.decision));
    println!(
        "  allocated: {} estimate",
        format_bytes(candidate.allocated_bytes_estimate)
    );
    println!("  logical:   {}", format_bytes(candidate.logical_bytes));
    println!("  recovery:  {}", candidate.recovery.detail);

    if let Some(command) = &candidate.recovery.command {
        println!("  command:   {command}");
    }

    println!("  evidence:");
    for evidence in &candidate.evidence {
        println!(
            "    {} {:<24} {}",
            evidence_symbol(&evidence.status),
            evidence.code,
            evidence.detail
        );
    }

    Ok(())
}

fn run_report(
    path: PathBuf,
    format: OutputFormat,
    output: Option<PathBuf>,
    save: bool,
    copy: bool,
    redact_home: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = path.canonicalize()?;
    let candidates = scan(&root, &ScanOptions::default())?;
    let home = env::var_os("HOME").map(PathBuf::from);
    let redaction_root = redact_home.then_some(home.as_deref()).flatten();
    let report = Report::from_candidates(&root, &candidates, redaction_root);
    let rendered = render(&report, format)?;

    let mut emitted = false;

    if let Some(output) = output {
        atomic_write(&output, rendered.as_bytes())?;
        eprintln!("wrote report to {}", output.display());
        emitted = true;
    }

    if save {
        let output = downloads_report_path(&report, format)?;
        atomic_write(&output, rendered.as_bytes())?;
        eprintln!("saved report to {}", output.display());
        emitted = true;
    }

    if copy {
        copy_to_clipboard(&rendered)?;
        eprintln!("copied {} report to clipboard", format.as_str());
        emitted = true;
    }

    if !emitted {
        print!("{rendered}");
        if !rendered.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn downloads_report_path(report: &Report, format: OutputFormat) -> io::Result<PathBuf> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;

    Ok(home.join("Downloads").join(format!(
        "sweep-report-{}.{}",
        report.created_unix_seconds,
        format.extension()
    )))
}

fn copy_to_clipboard(content: &str) -> io::Result<()> {
    if env::consts::OS != "macos" {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "clipboard copy is currently supported on macOS only",
        ));
    }

    let mut child = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let Some(mut stdin) = child.stdin.take() else {
        return Err(io::Error::other("could not open pbcopy stdin"));
    };

    stdin.write_all(content.as_bytes())?;
    drop(stdin);

    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("pbcopy exited with {status}")))
    }
}

fn run_snapshot(path: PathBuf, output: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let root = path.canonicalize()?;
    let scan = scan_with_diagnostics(&root, &ScanOptions::default())?;
    let snapshot = Snapshot::from_candidates(
        &root,
        &scan.candidates,
        scan.discovery_complete,
        scan.discovery_error_count,
    );
    let json = serde_json::to_string_pretty(&snapshot)?;
    let output = match output {
        Some(output) => output,
        None => default_snapshot_path(&snapshot)?,
    };

    atomic_write(&output, json.as_bytes())?;
    eprintln!(
        "saved {} candidates ({}) to {}",
        snapshot.summary.candidate_count,
        format_bytes(snapshot.summary.allocated_bytes_estimate),
        output.display()
    );

    if !snapshot.complete {
        eprintln!(
            "warning: snapshot is incomplete ({} discovery errors); diffs may omit filesystem state",
            snapshot.discovery_error_count
        );
    }

    Ok(())
}

fn run_diff(
    before: PathBuf,
    after: PathBuf,
    format: SnapshotDiffFormat,
    output: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let before_bytes = fs::read(&before)?;
    let after_bytes = fs::read(&after)?;
    let before_snapshot: Snapshot = serde_json::from_slice(&before_bytes)?;
    let after_snapshot: Snapshot = serde_json::from_slice(&after_bytes)?;
    let diff = before_snapshot.diff(&after_snapshot)?;
    let rendered = render_snapshot_diff(&diff, format)?;

    if let Some(output) = output {
        atomic_write(&output, rendered.as_bytes())?;
        eprintln!("wrote snapshot diff to {}", output.display());
    } else {
        print!("{rendered}");
        if !rendered.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn default_snapshot_path(snapshot: &Snapshot) -> io::Result<PathBuf> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    let directory = home.join("Library/Application Support/Sweep/snapshots");
    let stem = format!("sweep-snapshot-{}", snapshot.created_unix_seconds);
    let direct = directory.join(format!("{stem}.sweep.json"));

    if !direct.exists() {
        return Ok(direct);
    }

    let mut suffix = 1_u64;
    loop {
        let candidate = directory.join(format!("{stem}-{suffix}.sweep.json"));
        if !candidate.exists() {
            return Ok(candidate);
        }

        suffix = suffix.checked_add(1).ok_or_else(|| {
            io::Error::other("snapshot filename suffix space exhausted")
        })?;
    }
}

fn run_plan(path: PathBuf, output: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let root = path.canonicalize()?;
    let candidates = scan(&root, &ScanOptions::default())?;
    let plan = Plan::from_candidates(root, candidates);
    let json = serde_json::to_string_pretty(&plan)?;

    if let Some(output) = output {
        atomic_write(&output, json.as_bytes())?;
        eprintln!(
            "wrote {} safe candidates ({}) to {}",
            plan.candidates.len(),
            format_bytes(plan.allocated_bytes_estimate()),
            output.display()
        );
    } else {
        println!("{json}");
    }

    Ok(())
}

fn run_doctor(json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let output = DoctorOutput {
        os: env::consts::OS.to_owned(),
        architecture: env::consts::ARCH.to_owned(),
        git: command_probe("git", &["--version"]),
        root_filesystem: filesystem_probe(),
        mutation_enabled: false,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("Sweep doctor");
    println!("  os:            {}", output.os);
    println!("  architecture:  {}", output.architecture);
    println!(
        "  git:           {} ({})",
        output.git.status, output.git.detail
    );
    println!(
        "  filesystem:    {} ({})",
        output.root_filesystem.status, output.root_filesystem.detail
    );
    println!("  mutation:      disabled in this release");
    Ok(())
}

fn print_scan(root: &Path, candidates: &[Candidate]) {
    println!("Sweep scan");
    println!("  root: {}", root.display());
    println!();

    if candidates.is_empty() {
        println!("No supported candidates found.");
        return;
    }

    println!(
        "{:<10} {:>12}  {:<16}  PATH",
        "DECISION", "ALLOCATED", "KIND"
    );

    for candidate in candidates {
        println!(
            "{:<10} {:>12}  {:<16}  {}",
            decision_name(candidate.decision),
            format_bytes(candidate.allocated_bytes_estimate),
            kind_name(candidate),
            candidate.path.display()
        );
    }

    let safe: u64 = candidates
        .iter()
        .filter(|candidate| candidate.decision == Decision::Safe)
        .map(|candidate| candidate.allocated_bytes_estimate)
        .sum();

    println!();
    println!("Safe allocated-size estimate: {}", format_bytes(safe));
    println!("No files were modified.");
}

fn kind_name(candidate: &Candidate) -> &'static str {
    use sweep_core::CandidateKind;

    match candidate.kind {
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

fn evidence_symbol(status: &sweep_core::EvidenceStatus) -> &'static str {
    use sweep_core::EvidenceStatus;

    match status {
        EvidenceStatus::Proven => "+",
        EvidenceStatus::Refuted => "!",
        EvidenceStatus::Unknown => "?",
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

fn command_probe(command: &str, args: &[&str]) -> Probe {
    match Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => Probe {
            status: String::from("ok"),
            detail: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        },
        Ok(output) => Probe {
            status: String::from("unavailable"),
            detail: format!("exit status {}", output.status),
        },
        Err(error) => Probe {
            status: String::from("unavailable"),
            detail: error.to_string(),
        },
    }
}

fn filesystem_probe() -> Probe {
    if env::consts::OS != "macos" {
        return Probe {
            status: String::from("unsupported"),
            detail: String::from("filesystem type probe is macOS-specific"),
        };
    }

    let diskutil = match Command::new("/usr/sbin/diskutil")
        .args(["info", "-plist", "/"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            return Probe {
                status: String::from("unknown"),
                detail: format!("diskutil exited with {}", output.status),
            };
        }
        Err(error) => {
            return Probe {
                status: String::from("unknown"),
                detail: error.to_string(),
            };
        }
    };

    let mut plutil = match Command::new("/usr/bin/plutil")
        .args(["-extract", "FilesystemType", "raw", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return Probe {
                status: String::from("unknown"),
                detail: error.to_string(),
            };
        }
    };

    let Some(mut stdin) = plutil.stdin.take() else {
        return Probe {
            status: String::from("unknown"),
            detail: String::from("could not open plutil stdin"),
        };
    };

    if stdin.write_all(&diskutil.stdout).is_err() {
        return Probe {
            status: String::from("unknown"),
            detail: String::from("could not pass diskutil plist to plutil"),
        };
    }
    drop(stdin);

    match plutil.wait_with_output() {
        Ok(output) if output.status.success() => Probe {
            status: String::from("ok"),
            detail: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        },
        Ok(output) => Probe {
            status: String::from("unknown"),
            detail: format!("plutil exited with {}", output.status),
        },
        Err(error) => Probe {
            status: String::from("unknown"),
            detail: error.to_string(),
        },
    }
}

fn lexical_absolute(path: PathBuf) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("sweep-plan"),
        std::process::id()
    ));

    let mut file = fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    fs::rename(temporary, path)
}
