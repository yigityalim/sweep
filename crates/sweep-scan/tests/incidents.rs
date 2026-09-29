use std::{fs, process::Command};

use sweep_core::{CandidateKind, Decision};
use sweep_scan::{ScanOptions, classify_path, scan};
use tempfile::tempdir;

fn init_git(path: &std::path::Path) {
    let status = Command::new("git")
        .arg("init")
        .arg("-q")
        .arg(path)
        .status()
        .expect("git must be available for incident tests");
    assert!(status.success());
}

#[test]
fn rust_target_with_anchor_keypair_is_protected() {
    let repo = tempdir().unwrap();
    init_git(repo.path());

    fs::write(
        repo.path().join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();
    fs::write(repo.path().join(".gitignore"), "/target\n").unwrap();
    fs::create_dir_all(repo.path().join("target/deploy")).unwrap();
    fs::write(
        repo.path().join("target/deploy/demo-keypair.json"),
        "[1,2,3]",
    )
    .unwrap();

    let candidate = classify_path(&repo.path().join("target"));

    assert_eq!(candidate.kind, CandidateKind::RustTarget);
    assert_eq!(candidate.decision, Decision::Protected);
}

#[test]
fn authored_claude_memory_is_not_a_candidate() {
    let home = tempdir().unwrap();
    let memory = home.path().join(".claude/projects/project/memory");
    fs::create_dir_all(&memory).unwrap();
    fs::write(memory.join("MEMORY.md"), "authored state").unwrap();

    let candidates = scan(home.path(), &ScanOptions::default()).unwrap();

    assert!(candidates.is_empty());
}

#[test]
fn xdg_local_root_is_not_a_candidate() {
    let home = tempdir().unwrap();
    fs::create_dir_all(home.path().join(".local/bin")).unwrap();
    fs::write(home.path().join(".local/bin/tool"), "binary").unwrap();

    let candidates = scan(home.path(), &ScanOptions::default()).unwrap();

    assert!(candidates.is_empty());
}

#[test]
fn ignored_node_modules_is_safe() {
    let repo = tempdir().unwrap();
    init_git(repo.path());

    fs::write(repo.path().join("package.json"), "{}").unwrap();
    fs::write(repo.path().join("package-lock.json"), "{}").unwrap();
    fs::write(repo.path().join(".gitignore"), "/node_modules\n").unwrap();
    fs::create_dir_all(repo.path().join("node_modules/pkg")).unwrap();
    fs::write(
        repo.path().join("node_modules/pkg/index.js"),
        "module.exports = 1",
    )
    .unwrap();

    let candidate = classify_path(&repo.path().join("node_modules"));

    assert_eq!(candidate.kind, CandidateKind::NodeModules);
    assert_eq!(candidate.decision, Decision::Safe);
}

#[test]
fn tracked_candidate_is_protected() {
    let repo = tempdir().unwrap();
    init_git(repo.path());

    fs::write(
        repo.path().join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();
    fs::create_dir_all(repo.path().join("target")).unwrap();
    fs::write(repo.path().join("target/authored.txt"), "keep").unwrap();

    let add = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["add", "target/authored.txt"])
        .status()
        .unwrap();
    assert!(add.success());

    let candidate = classify_path(&repo.path().join("target"));

    assert_eq!(candidate.decision, Decision::Protected);
}

#[test]
fn ignored_node_modules_without_lockfile_requires_review() {
    let repo = tempdir().unwrap();
    init_git(repo.path());

    fs::write(repo.path().join("package.json"), "{}").unwrap();
    fs::write(repo.path().join(".gitignore"), "/node_modules\n").unwrap();
    fs::create_dir_all(repo.path().join("node_modules/pkg")).unwrap();
    fs::write(
        repo.path().join("node_modules/pkg/index.js"),
        "module.exports = 1",
    )
    .unwrap();

    let candidate = classify_path(&repo.path().join("node_modules"));

    assert_eq!(candidate.kind, CandidateKind::NodeModules);
    assert_eq!(candidate.decision, Decision::Review);
}

#[test]
fn workspace_node_modules_inherits_repository_lockfile() {
    let repo = tempdir().unwrap();
    init_git(repo.path());

    fs::write(repo.path().join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();

    let package = repo.path().join("packages/ui");
    fs::create_dir_all(package.join("node_modules/pkg")).unwrap();
    fs::write(package.join("package.json"), "{}").unwrap();
    fs::write(
        package.join("node_modules/pkg/index.js"),
        "module.exports = 1",
    )
    .unwrap();

    let candidate = classify_path(&package.join("node_modules"));

    assert_eq!(candidate.kind, CandidateKind::NodeModules);
    assert_eq!(candidate.decision, Decision::Safe);
    assert_eq!(
        candidate.recovery.command.as_deref(),
        Some("pnpm install --frozen-lockfile")
    );
}

#[test]
fn ignored_tagged_python_tool_cache_is_safe() {
    let repo = tempdir().unwrap();
    init_git(repo.path());

    fs::write(
        repo.path().join("pyproject.toml"),
        "[project]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();
    fs::write(repo.path().join(".gitignore"), "/.ruff_cache\n").unwrap();
    fs::create_dir(repo.path().join(".ruff_cache")).unwrap();
    fs::write(
        repo.path().join(".ruff_cache/CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();

    let candidate = classify_path(&repo.path().join(".ruff_cache"));

    assert_eq!(candidate.kind, CandidateKind::RuffCache);
    assert_eq!(candidate.decision, Decision::Safe);
}

#[test]
fn python_virtual_environment_is_review_only() {
    let repo = tempdir().unwrap();
    init_git(repo.path());

    fs::write(
        repo.path().join("pyproject.toml"),
        "[project]\nname='demo'\nversion='0.1.0'\n",
    )
    .unwrap();
    fs::write(repo.path().join(".gitignore"), "/.venv\n").unwrap();
    fs::create_dir(repo.path().join(".venv")).unwrap();
    fs::write(repo.path().join(".venv/pyvenv.cfg"), "home = /usr/bin\n").unwrap();

    let candidate = classify_path(&repo.path().join(".venv"));

    assert_eq!(candidate.kind, CandidateKind::PythonVirtualEnv);
    assert_eq!(candidate.decision, Decision::Review);
}
