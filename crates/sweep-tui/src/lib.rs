#![forbid(unsafe_code)]

mod app;
mod preview;
mod theme;
mod ui;

use std::{
    env, io,
    path::{Path, PathBuf},
};

pub fn default_root() -> io::Result<PathBuf> {
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        let developer = home.join("Developer");
        if developer.is_dir() {
            return developer.canonicalize();
        }
    }

    env::current_dir()?.canonicalize()
}

pub fn run(root: PathBuf) -> io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let mut app = app::App::new(root)?;
    let run_result = app.run(&mut terminal);
    let restore_result = ratatui::try_restore();

    match (run_result, restore_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

pub(crate) fn display_path(path: &Path) -> String {
    let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
        return path.to_string_lossy().into_owned();
    };

    if path == home {
        return String::from("~");
    }

    match path.strip_prefix(&home) {
        Ok(relative) => format!("~/{}", relative.to_string_lossy()),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}
