use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitState {
    IgnoredAndUntracked,
    Tracked,
    NotIgnored,
    NotRepository,
    Unavailable(String),
}

pub fn inspect(path: &Path) -> GitState {
    let Some(repo) = find_repo_root(path) else {
        return GitState::NotRepository;
    };

    let Ok(relative) = path.strip_prefix(&repo) else {
        return GitState::Unavailable(String::from("candidate is outside repository root"));
    };

    match git_status(&repo, ["ls-files", "--error-unmatch", "--"], Some(relative)) {
        Ok(true) => return GitState::Tracked,
        Ok(false) => {}
        Err(error) => return GitState::Unavailable(error),
    }

    match git_status(&repo, ["check-ignore", "-q", "--"], Some(relative)) {
        Ok(true) => GitState::IgnoredAndUntracked,
        Ok(false) => GitState::NotIgnored,
        Err(error) => GitState::Unavailable(error),
    }
}

fn find_repo_root(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent()?.to_path_buf();

    loop {
        if current.join(".git").exists() {
            return Some(current);
        }

        if !current.pop() {
            return None;
        }
    }
}

fn git_status<const N: usize>(
    repo: &Path,
    args: [&str; N],
    path: Option<&Path>,
) -> Result<bool, String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(path) = path {
        command.arg(path);
    }

    let status = command
        .status()
        .map_err(|error| format!("git could not be executed: {error}"))?;

    Ok(status.success())
}
