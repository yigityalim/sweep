use std::{
    cmp::Ordering,
    collections::BTreeSet,
    env, io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    widgets::TableState,
};
use sweep_report::{OutputFormat, Report, ReportCandidate, SnapshotDiff};
use sweep_scan::{ScanOptions, scan_with_diagnostics};
use tachyonfx::{EffectManager, Interpolation, Motion, fx};

use crate::{
    display_path,
    preview::{
        BrowseEntry, GrowthData, copy_report, copy_to_clipboard, history_directory, list_directory,
        load_growth, reveal_in_finder, save_report,
    },
    theme::Theme,
    ui,
};

const ACTIVE_FRAME_TIME: Duration = Duration::from_millis(33);
const IDLE_FRAME_TIME: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum View {
    Candidates,
    Browse,
    Growth,
    History,
}

impl View {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Candidates => "candidates",
            Self::Browse => "browse",
            Self::Growth => "growth",
            Self::History => "history",
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Candidates => Self::Browse,
            Self::Browse => Self::Growth,
            Self::Growth => Self::History,
            Self::History => Self::Candidates,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CandidateFilter {
    All,
    Safe,
    Review,
    Protected,
}

impl CandidateFilter {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Safe => "safe",
            Self::Review => "review",
            Self::Protected => "protected",
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::All => Self::Safe,
            Self::Safe => Self::Review,
            Self::Review => Self::Protected,
            Self::Protected => Self::All,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SortKey {
    Size,
    Decision,
    Kind,
    Path,
}

impl SortKey {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Decision => "decision",
            Self::Kind => "type",
            Self::Path => "path",
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Size => Self::Decision,
            Self::Decision => Self::Kind,
            Self::Kind => Self::Path,
            Self::Path => Self::Size,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputMode {
    Normal,
    Search,
    Path,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Overlay {
    Help,
    Inspect,
    Palette,
    PathInput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandFamily {
    Yank,
    Save,
}

impl CommandFamily {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Yank => "YANK",
            Self::Save => "SAVE",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaletteAction {
    PreviewClean,
    RevealFinder,
    ChangeScope,
    UseBrowseAsScope,
    SaveText,
    SaveMarkdown,
    SaveJson,
    SaveToml,
    CopyText,
    CopyMarkdown,
    CopyJson,
    CopyToml,
    OpenBrowse,
    OpenGrowth,
    OpenHistory,
    Rescan,
}

impl PaletteAction {
    pub(crate) const ALL: [Self; 16] = [
        Self::PreviewClean,
        Self::RevealFinder,
        Self::ChangeScope,
        Self::UseBrowseAsScope,
        Self::SaveText,
        Self::SaveMarkdown,
        Self::SaveJson,
        Self::SaveToml,
        Self::CopyText,
        Self::CopyMarkdown,
        Self::CopyJson,
        Self::CopyToml,
        Self::OpenBrowse,
        Self::OpenGrowth,
        Self::OpenHistory,
        Self::Rescan,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::PreviewClean => "Preview safe cleanup plan",
            Self::RevealFinder => "Reveal selected path in Finder",
            Self::ChangeScope => "Change scan scope…",
            Self::UseBrowseAsScope => "Use browsed directory as scan scope",
            Self::SaveText => "Save text report",
            Self::SaveMarkdown => "Save Markdown report",
            Self::SaveJson => "Save JSON report",
            Self::SaveToml => "Save TOML report",
            Self::CopyText => "Copy text report",
            Self::CopyMarkdown => "Copy Markdown report",
            Self::CopyJson => "Copy JSON report",
            Self::CopyToml => "Copy TOML report",
            Self::OpenBrowse => "Open file browser",
            Self::OpenGrowth => "Open snapshot growth",
            Self::OpenHistory => "Open cleanup history",
            Self::Rescan => "Rescan current scope",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CleanPlan {
    pub(crate) requested_count: usize,
    pub(crate) included: Vec<ReportCandidate>,
    pub(crate) excluded: Vec<ReportCandidate>,
    pub(crate) allocated_bytes_estimate: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum Drawer {
    CleanPlan(CleanPlan),
    PreviewReceipt(CleanPlan),
}

enum ScanMessage {
    Finished {
        report: Report,
        elapsed: Duration,
        discovery_error_count: usize,
    },
    Failed(String),
}

pub(crate) struct App {
    pub(crate) root: PathBuf,
    pub(crate) report: Option<Report>,
    pub(crate) table_state: TableState,
    pub(crate) browse_state: TableState,
    pub(crate) browse_path: PathBuf,
    pub(crate) browse_entries: Vec<BrowseEntry>,
    pub(crate) browse_error: Option<String>,
    pub(crate) growth: GrowthData,
    pub(crate) view: View,
    pub(crate) filter: CandidateFilter,
    pub(crate) sort: SortKey,
    pub(crate) query: String,
    pub(crate) path_input: String,
    pub(crate) input_mode: InputMode,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) drawer: Option<Drawer>,
    pub(crate) palette_index: usize,
    pub(crate) palette_query: String,
    pub(crate) inspect_scroll: u16,
    pub(crate) pending_family: Option<CommandFamily>,
    pub(crate) selected_paths: BTreeSet<String>,
    pub(crate) status_message: Option<String>,
    pub(crate) scanning: bool,
    pub(crate) scan_elapsed: Duration,
    pub(crate) last_scan_elapsed: Option<Duration>,
    pub(crate) last_error: Option<String>,
    pub(crate) discovery_error_count: usize,
    pub(crate) no_color: bool,
    pub(crate) ascii: bool,
    pub(crate) effects: EffectManager<()>,
    scan_receiver: Option<Receiver<ScanMessage>>,
    scan_started: Instant,
    last_frame: Instant,
    pending_g: bool,
    should_quit: bool,
    scope_history: Vec<PathBuf>,
    scope_history_index: usize,
}

impl App {
    pub(crate) fn new(root: PathBuf) -> io::Result<Self> {
        let root = root.canonicalize()?;
        let no_color = env::var_os("NO_COLOR").is_some()
            || env::var_os("TERM").is_some_and(|value| value == "dumb");
        let ascii = env::var_os("SWEEP_ASCII").is_some();
        let growth = load_growth(&root).unwrap_or_else(|error| GrowthData {
            diff: None,
            snapshot_count: 0,
            invalid_snapshot_count: 0,
            message: format!("Could not load snapshots: {error}"),
        });

        let mut app = Self {
            root: root.clone(),
            report: None,
            table_state: TableState::default(),
            browse_state: TableState::default(),
            browse_path: root.clone(),
            browse_entries: Vec::new(),
            browse_error: None,
            growth,
            view: View::Candidates,
            filter: CandidateFilter::All,
            sort: SortKey::Size,
            query: String::new(),
            path_input: String::new(),
            input_mode: InputMode::Normal,
            overlay: None,
            drawer: None,
            palette_index: 0,
            palette_query: String::new(),
            inspect_scroll: 0,
            pending_family: None,
            selected_paths: BTreeSet::new(),
            status_message: None,
            scanning: false,
            scan_elapsed: Duration::ZERO,
            last_scan_elapsed: None,
            last_error: None,
            discovery_error_count: 0,
            no_color,
            ascii,
            effects: EffectManager::default(),
            scan_receiver: None,
            scan_started: Instant::now(),
            last_frame: Instant::now(),
            pending_g: false,
            should_quit: false,
            scope_history: vec![root],
            scope_history_index: 0,
        };

        app.refresh_browser();
        app.start_scan()?;

        if !app.no_color {
            app.effects.add_effect(fx::sweep_in(
                Motion::LeftToRight,
                16,
                0,
                Theme::new(false).background(),
                (420, Interpolation::QuadOut),
            ));
        }

        Ok(app)
    }

    pub(crate) fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.should_quit {
            self.poll_scan();

            let now = Instant::now();
            let frame_delta = now.saturating_duration_since(self.last_frame);
            self.last_frame = now;
            if self.scanning {
                self.scan_elapsed = now.saturating_duration_since(self.scan_started);
            }

            terminal.draw(|frame| ui::render(frame, self, frame_delta))?;

            let timeout = if self.scanning || self.effects.is_running() {
                ACTIVE_FRAME_TIME
            } else {
                IDLE_FRAME_TIME
            };

            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key)?,
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
        }

        Ok(())
    }

    pub(crate) fn visible_indices(&self) -> Vec<usize> {
        let Some(report) = &self.report else {
            return Vec::new();
        };

        let query = self.query.trim().to_lowercase();
        let mut indices: Vec<_> = report
            .candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate_matches(candidate, self.filter, &query))
            .map(|(index, _)| index)
            .collect();

        indices.sort_by(|left, right| {
            compare_candidates(
                &report.candidates[*left],
                &report.candidates[*right],
                self.sort,
            )
        });

        indices
    }

    pub(crate) fn visible_browse_indices(&self) -> Vec<usize> {
        let query = self.query.trim().to_lowercase();
        self.browse_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| query.is_empty() || entry.name.to_lowercase().contains(&query))
            .map(|(index, _)| index)
            .collect()
    }

    pub(crate) fn selected_candidate(&self) -> Option<&ReportCandidate> {
        let report = self.report.as_ref()?;
        let indices = self.visible_indices();
        let selected = self.table_state.selected()?;
        let index = *indices.get(selected)?;
        report.candidates.get(index)
    }

    pub(crate) fn selected_browse_entry(&self) -> Option<&BrowseEntry> {
        let indices = self.visible_browse_indices();
        let selected = self.browse_state.selected()?;
        let index = *indices.get(selected)?;
        self.browse_entries.get(index)
    }

    pub(crate) fn root_label(&self) -> String {
        display_path(&self.root)
    }

    pub(crate) fn browse_path_label(&self) -> String {
        display_path(&self.browse_path)
    }

    pub(crate) fn history_path_label(&self) -> String {
        history_directory()
            .as_deref()
            .map(display_path)
            .unwrap_or_else(|| String::from("~/Library/Application Support/Sweep/history"))
    }

    pub(crate) fn growth_diff(&self) -> Option<&SnapshotDiff> {
        self.growth.diff.as_ref()
    }

    pub(crate) fn visible_palette_actions(&self) -> Vec<PaletteAction> {
        let query = self.palette_query.trim().to_lowercase();
        PaletteAction::ALL
            .iter()
            .copied()
            .filter(|action| query.is_empty() || action.label().to_lowercase().contains(&query))
            .collect()
    }

    pub(crate) fn candidate_decision_for_path(&self, path: &Path) -> Option<&str> {
        let display = display_path(path);
        self.report
            .as_ref()?
            .candidates
            .iter()
            .find(|candidate| candidate.path == display)
            .map(|candidate| candidate.decision.as_str())
    }

    fn start_scan(&mut self) -> io::Result<()> {
        if self.scanning {
            return Ok(());
        }

        let root = self.root.clone();
        let home = env::var_os("HOME").map(PathBuf::from);
        let (sender, receiver) = mpsc::channel();

        thread::Builder::new()
            .name(String::from("sweep-scan"))
            .spawn(move || {
                let started = Instant::now();
                let message = match scan_with_diagnostics(&root, &ScanOptions::default()) {
                    Ok(scan) => ScanMessage::Finished {
                        report: Report::from_candidates(&root, &scan.candidates, home.as_deref()),
                        elapsed: started.elapsed(),
                        discovery_error_count: scan.discovery_error_count,
                    },
                    Err(error) => ScanMessage::Failed(error.to_string()),
                };
                let _ = sender.send(message);
            })?;

        self.scanning = true;
        self.scan_elapsed = Duration::ZERO;
        self.scan_started = Instant::now();
        self.scan_receiver = Some(receiver);
        self.last_error = None;
        Ok(())
    }

    fn poll_scan(&mut self) {
        let Some(receiver) = self.scan_receiver.as_ref() else {
            return;
        };

        let message = match receiver.try_recv() {
            Ok(message) => Some(message),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(ScanMessage::Failed(String::from(
                "scan worker disconnected",
            ))),
        };

        let Some(message) = message else {
            return;
        };

        self.scan_receiver = None;
        self.scanning = false;

        match message {
            ScanMessage::Finished {
                report,
                elapsed,
                discovery_error_count,
            } => {
                self.report = Some(report);
                self.last_scan_elapsed = Some(elapsed);
                self.scan_elapsed = elapsed;
                self.discovery_error_count = discovery_error_count;
                self.selected_paths.clear();
                self.clamp_candidate_selection();

                if !self.no_color {
                    self.effects.add_effect(fx::fade_from_fg(
                        Theme::new(false).muted(),
                        (260, Interpolation::QuadOut),
                    ));
                }
            }
            ScanMessage::Failed(error) => {
                self.last_error = Some(error);
                self.scan_elapsed = self.scan_started.elapsed();
            }
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> io::Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(());
        }

        match self.input_mode {
            InputMode::Search => {
                self.on_search_key(key);
                return Ok(());
            }
            InputMode::Path => {
                self.on_path_key(key)?;
                return Ok(());
            }
            InputMode::Normal => {}
        }

        if self.handle_command_family_key(key) {
            return Ok(());
        }

        if self.handle_drawer_key(key) {
            return Ok(());
        }

        if self.handle_overlay_key(key)? {
            return Ok(());
        }

        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Left => {
                    self.navigate_scope_history(-1)?;
                    return Ok(());
                }
                KeyCode::Right => {
                    self.navigate_scope_history(1)?;
                    return Ok(());
                }
                _ => {}
            }
        }

        if key.code != KeyCode::Char('g') {
            self.pending_g = false;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.overlay = Some(Overlay::Help),
            KeyCode::Char(':') => {
                self.palette_index = 0;
                self.palette_query.clear();
                self.overlay = Some(Overlay::Palette);
            }
            KeyCode::Char('y') => self.pending_family = Some(CommandFamily::Yank),
            KeyCode::Char('s') => self.pending_family = Some(CommandFamily::Save),
            KeyCode::Char('1') => self.set_view(View::Candidates),
            KeyCode::Char('2') => self.set_view(View::Browse),
            KeyCode::Char('3') => self.set_view(View::Growth),
            KeyCode::Char('4') => self.set_view(View::History),
            KeyCode::Tab => self.set_view(self.view.next()),
            KeyCode::Char('o') => self.reveal_selected(),
            KeyCode::Char('c') if self.view == View::Candidates => self.open_clean_preview(),
            KeyCode::Char('R') => {
                if self.view == View::Growth {
                    self.reload_growth();
                } else {
                    self.start_scan()?;
                }
            }
            KeyCode::Char('/') if matches!(self.view, View::Candidates | View::Browse) => {
                self.query.clear();
                self.input_mode = InputMode::Search;
            }
            KeyCode::Esc if !self.query.is_empty() => {
                self.query.clear();
                self.clamp_active_selection();
            }
            _ => self.on_view_key(key)?,
        }

        Ok(())
    }

    fn on_view_key(&mut self, key: KeyEvent) -> io::Result<()> {
        match self.view {
            View::Candidates => self.on_candidate_key(key),
            View::Browse => self.on_browse_key(key),
            View::Growth | View::History => Ok(()),
        }
    }

    fn on_candidate_key(&mut self, key: KeyEvent) -> io::Result<()> {
        match key.code {
            KeyCode::Char('e') | KeyCode::Enter => {
                if self.selected_candidate().is_some() {
                    self.inspect_scroll = 0;
                    self.overlay = Some(Overlay::Inspect);
                }
            }
            KeyCode::Char(' ') => self.toggle_candidate_mark(),
            KeyCode::Char('f') => {
                self.filter = self.filter.next();
                self.clamp_candidate_selection();
            }
            KeyCode::Char('S') => {
                self.sort = self.sort.next();
                self.clamp_candidate_selection();
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_candidate_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_candidate_selection(-1),
            KeyCode::Home => self.select_candidate_first(),
            KeyCode::End | KeyCode::Char('G') => self.select_candidate_last(),
            KeyCode::Char('g') => {
                if self.pending_g {
                    self.select_candidate_first();
                    self.pending_g = false;
                } else {
                    self.pending_g = true;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn on_browse_key(&mut self, key: KeyEvent) -> io::Result<()> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_browse_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_browse_selection(-1),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => self.browse_parent(),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => self.enter_browse_directory(),
            KeyCode::Home => self.select_browse_first(),
            KeyCode::End | KeyCode::Char('G') => self.select_browse_last(),
            _ => {}
        }
        Ok(())
    }

    fn on_search_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
            self.query.clear();
            self.clamp_active_selection();
            return;
        }

        match key.code {
            KeyCode::Enter | KeyCode::Esc => self.input_mode = InputMode::Normal,
            KeyCode::Backspace => {
                self.query.pop();
                self.clamp_active_selection();
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.push(character);
                self.clamp_active_selection();
            }
            _ => {}
        }
    }

    fn on_path_key(&mut self, key: KeyEvent) -> io::Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
            self.path_input.clear();
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.overlay = None;
            }
            KeyCode::Enter => {
                let requested = self.expand_input_path();
                match requested.and_then(|path| path.canonicalize()) {
                    Ok(path) if path.is_dir() => {
                        self.input_mode = InputMode::Normal;
                        self.overlay = None;
                        self.apply_new_scope(path, true)?;
                    }
                    Ok(_) => {
                        self.status_message = Some(String::from("Scope must be a directory."));
                    }
                    Err(error) => {
                        self.status_message = Some(format!("Could not change scope: {error}"));
                    }
                }
            }
            KeyCode::Backspace => {
                self.path_input.pop();
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.path_input.push(character);
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_command_family_key(&mut self, key: KeyEvent) -> bool {
        let Some(family) = self.pending_family else {
            return false;
        };

        self.pending_family = None;

        match key.code {
            KeyCode::Esc => {}
            KeyCode::Char('y') if family == CommandFamily::Yank => {
                self.copy_current_report(OutputFormat::Text);
            }
            KeyCode::Char('p') if family == CommandFamily::Yank => {
                self.copy_selected_path();
            }
            KeyCode::Char('m') if family == CommandFamily::Yank => {
                self.copy_current_report(OutputFormat::Markdown);
            }
            KeyCode::Char('j') if family == CommandFamily::Yank => {
                self.copy_current_report(OutputFormat::Json);
            }
            KeyCode::Char('t') if family == CommandFamily::Yank => {
                self.copy_current_report(OutputFormat::Toml);
            }
            KeyCode::Char('y') if family == CommandFamily::Save => {
                self.save_current_report(OutputFormat::Text);
            }
            KeyCode::Char('m') if family == CommandFamily::Save => {
                self.save_current_report(OutputFormat::Markdown);
            }
            KeyCode::Char('j') if family == CommandFamily::Save => {
                self.save_current_report(OutputFormat::Json);
            }
            KeyCode::Char('t') if family == CommandFamily::Save => {
                self.save_current_report(OutputFormat::Toml);
            }
            _ => {
                self.status_message = Some(format!(
                    "Unknown {} command. Press ? for the keymap.",
                    family.label().to_lowercase()
                ));
            }
        }

        true
    }

    fn handle_drawer_key(&mut self, key: KeyEvent) -> bool {
        let Some(drawer) = self.drawer.clone() else {
            return false;
        };

        match drawer {
            Drawer::CleanPlan(plan) => match key.code {
                KeyCode::Esc => self.drawer = None,
                KeyCode::Enter => {
                    self.drawer = Some(Drawer::PreviewReceipt(plan));
                    self.status_message =
                        Some(String::from("Preview complete. No files were changed."));
                }
                _ => {}
            },
            Drawer::PreviewReceipt(_) => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.drawer = None,
                _ => {}
            },
        }

        true
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) -> io::Result<bool> {
        let Some(overlay) = self.overlay else {
            return Ok(false);
        };

        match overlay {
            Overlay::Palette => {
                let visible = self.visible_palette_actions();
                match key.code {
                    KeyCode::Esc => self.overlay = None,
                    KeyCode::Down => {
                        if !visible.is_empty() {
                            self.palette_index =
                                (self.palette_index + 1).min(visible.len() - 1);
                        }
                    }
                    KeyCode::Up => {
                        self.palette_index = self.palette_index.saturating_sub(1);
                    }
                    KeyCode::Home => self.palette_index = 0,
                    KeyCode::End => {
                        self.palette_index = visible.len().saturating_sub(1);
                    }
                    KeyCode::Backspace => {
                        self.palette_query.pop();
                        self.palette_index = 0;
                    }
                    KeyCode::Enter => {
                        if let Some(action) = visible.get(self.palette_index).copied() {
                            self.overlay = None;
                            self.execute_palette_action(action)?;
                        }
                    }
                    KeyCode::Char(character)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        self.palette_query.push(character);
                        self.palette_index = 0;
                    }
                    _ => {}
                }
            }
            Overlay::Help => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') => self.overlay = None,
                _ => {}
            },
            Overlay::Inspect => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.overlay = None,
                KeyCode::Char('j') | KeyCode::Down => {
                    self.inspect_scroll = self.inspect_scroll.saturating_add(1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.inspect_scroll = self.inspect_scroll.saturating_sub(1);
                }
                KeyCode::PageDown => {
                    self.inspect_scroll = self.inspect_scroll.saturating_add(8);
                }
                KeyCode::PageUp => {
                    self.inspect_scroll = self.inspect_scroll.saturating_sub(8);
                }
                KeyCode::Home => self.inspect_scroll = 0,
                _ => {}
            },
            Overlay::PathInput => {}
        }

        Ok(true)
    }

    fn execute_palette_action(&mut self, action: PaletteAction) -> io::Result<()> {
        match action {
            PaletteAction::PreviewClean => {
                self.set_view(View::Candidates);
                self.open_clean_preview();
            }
            PaletteAction::RevealFinder => self.reveal_selected(),
            PaletteAction::ChangeScope => {
                self.path_input = self.root_label();
                self.input_mode = InputMode::Path;
                self.overlay = Some(Overlay::PathInput);
            }
            PaletteAction::UseBrowseAsScope => {
                if self.view == View::Browse {
                    self.apply_new_scope(self.browse_path.clone(), true)?;
                } else {
                    self.status_message =
                        Some(String::from("Open Browse first to choose a directory."));
                }
            }
            PaletteAction::SaveText => self.save_current_report(OutputFormat::Text),
            PaletteAction::SaveMarkdown => self.save_current_report(OutputFormat::Markdown),
            PaletteAction::SaveJson => self.save_current_report(OutputFormat::Json),
            PaletteAction::SaveToml => self.save_current_report(OutputFormat::Toml),
            PaletteAction::CopyText => self.copy_current_report(OutputFormat::Text),
            PaletteAction::CopyMarkdown => self.copy_current_report(OutputFormat::Markdown),
            PaletteAction::CopyJson => self.copy_current_report(OutputFormat::Json),
            PaletteAction::CopyToml => self.copy_current_report(OutputFormat::Toml),
            PaletteAction::OpenBrowse => self.set_view(View::Browse),
            PaletteAction::OpenGrowth => self.set_view(View::Growth),
            PaletteAction::OpenHistory => self.set_view(View::History),
            PaletteAction::Rescan => self.start_scan()?,
        }
        Ok(())
    }

    fn set_view(&mut self, view: View) {
        self.view = view;
        self.query.clear();
        self.input_mode = InputMode::Normal;
        self.overlay = None;
        self.drawer = None;
        self.pending_family = None;
        self.inspect_scroll = 0;

        match view {
            View::Candidates => self.clamp_candidate_selection(),
            View::Browse => {
                self.refresh_browser();
                self.clamp_browse_selection();
            }
            View::Growth => self.reload_growth(),
            View::History => {}
        }
    }

    fn reload_growth(&mut self) {
        self.growth = load_growth(&self.root).unwrap_or_else(|error| GrowthData {
            diff: None,
            snapshot_count: 0,
            invalid_snapshot_count: 0,
            message: format!("Could not load snapshots: {error}"),
        });
        self.status_message = Some(self.growth.message.clone());
    }

    fn refresh_browser(&mut self) {
        match list_directory(&self.browse_path) {
            Ok(entries) => {
                self.browse_entries = entries;
                self.browse_error = None;
                self.clamp_browse_selection();
            }
            Err(error) => {
                self.browse_entries.clear();
                self.browse_state.select(None);
                self.browse_error = Some(error.to_string());
            }
        }
    }

    fn browse_parent(&mut self) {
        if self.browse_path == self.root {
            self.status_message = Some(String::from("Browse is already at the scan scope root."));
            return;
        }

        let Some(parent) = self.browse_path.parent() else {
            return;
        };
        if !parent.starts_with(&self.root) {
            return;
        }

        self.browse_path = parent.to_path_buf();
        self.query.clear();
        self.refresh_browser();
    }

    fn enter_browse_directory(&mut self) {
        let Some(entry) = self.selected_browse_entry().cloned() else {
            return;
        };

        if entry.is_symlink {
            self.status_message = Some(String::from(
                "Symlink traversal is disabled in Sweep Browse.",
            ));
            return;
        }

        if !entry.is_dir {
            self.status_message = Some(String::from("Selected entry is not a directory."));
            return;
        }

        self.browse_path = entry.path;
        self.query.clear();
        self.refresh_browser();
    }

    fn reveal_selected(&mut self) {
        let path = match self.view {
            View::Candidates => self
                .selected_candidate()
                .and_then(|candidate| self.expand_report_path(&candidate.path)),
            View::Browse => self
                .selected_browse_entry()
                .map(|entry| entry.path.clone())
                .or_else(|| Some(self.browse_path.clone())),
            View::Growth | View::History => Some(self.root.clone()),
        };

        let Some(path) = path else {
            self.status_message = Some(String::from("No path selected."));
            return;
        };

        match reveal_in_finder(&path) {
            Ok(()) => {
                self.status_message = Some(format!("Revealed {} in Finder.", display_path(&path)));
            }
            Err(error) => {
                self.status_message = Some(format!("Finder reveal failed: {error}"));
            }
        }
    }

    fn save_current_report(&mut self, format: OutputFormat) {
        let Some(report) = self.report.as_ref() else {
            self.status_message = Some(String::from("A completed scan is required first."));
            return;
        };

        match save_report(report, format) {
            Ok(path) => {
                self.status_message = Some(format!("Saved report to {}.", display_path(&path)));
            }
            Err(error) => {
                self.status_message = Some(format!("Report save failed: {error}"));
            }
        }
    }

    fn copy_current_report(&mut self, format: OutputFormat) {
        let Some(report) = self.report.as_ref() else {
            self.status_message = Some(String::from("A completed scan is required first."));
            return;
        };

        match copy_report(report, format) {
            Ok(()) => {
                self.status_message = Some(format!(
                    "Copied {} report to clipboard.",
                    format.as_str()
                ));
            }
            Err(error) => {
                self.status_message = Some(format!("Clipboard copy failed: {error}"));
            }
        }
    }

    fn copy_selected_path(&mut self) {
        let path = match self.view {
            View::Candidates => self
                .selected_candidate()
                .and_then(|candidate| self.expand_report_path(&candidate.path)),
            View::Browse => self
                .selected_browse_entry()
                .map(|entry| entry.path.clone())
                .or_else(|| Some(self.browse_path.clone())),
            View::Growth | View::History => Some(self.root.clone()),
        };

        let Some(path) = path else {
            self.status_message = Some(String::from("No path selected."));
            return;
        };

        let value = path.to_string_lossy().into_owned();
        match copy_to_clipboard(&value) {
            Ok(()) => {
                self.status_message =
                    Some(format!("Copied path: {}", display_path(&path)));
            }
            Err(error) => {
                self.status_message = Some(format!("Clipboard copy failed: {error}"));
            }
        }
    }

    fn open_clean_preview(&mut self) {
        let Some(report) = self.report.as_ref() else {
            self.status_message = Some(String::from("A completed scan is required first."));
            return;
        };

        let current = self
            .selected_candidate()
            .map(|candidate| candidate.path.as_str());
        let Some(plan) = build_clean_plan(report, &self.selected_paths, current) else {
            self.status_message = Some(String::from("Select at least one candidate first."));
            return;
        };

        self.drawer = Some(Drawer::CleanPlan(plan));
    }

    fn toggle_candidate_mark(&mut self) {
        let Some(path) = self
            .selected_candidate()
            .map(|candidate| candidate.path.clone())
        else {
            return;
        };

        if !self.selected_paths.insert(path.clone()) {
            self.selected_paths.remove(&path);
        }
    }

    fn apply_new_scope(&mut self, root: PathBuf, record_history: bool) -> io::Result<()> {
        if root == self.root {
            self.status_message = Some(String::from("Already scanning this scope."));
            return Ok(());
        }

        if record_history {
            self.scope_history.truncate(self.scope_history_index + 1);
            self.scope_history.push(root.clone());
            self.scope_history_index = self.scope_history.len() - 1;
        }

        self.root = root.clone();
        self.report = None;
        self.last_error = None;
        self.discovery_error_count = 0;
        self.table_state.select(None);
        self.selected_paths.clear();
        self.drawer = None;
        self.query.clear();
        self.browse_path = root;
        self.browse_state.select(None);
        self.refresh_browser();
        self.reload_growth();

        self.scan_receiver = None;
        self.scanning = false;
        self.start_scan()?;
        self.status_message = Some(format!("Scanning new scope: {}", self.root_label()));
        Ok(())
    }

    fn navigate_scope_history(&mut self, delta: isize) -> io::Result<()> {
        let next = if delta.is_negative() {
            self.scope_history_index
                .saturating_sub(delta.unsigned_abs())
        } else {
            self.scope_history_index
                .saturating_add(delta as usize)
                .min(self.scope_history.len() - 1)
        };

        if next == self.scope_history_index {
            return Ok(());
        }

        self.scope_history_index = next;
        let root = self.scope_history[next].clone();
        self.apply_new_scope(root, false)
    }

    fn expand_input_path(&self) -> io::Result<PathBuf> {
        let input = self.path_input.trim();
        if input.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "scope path is empty",
            ));
        }

        if input == "~" || input.starts_with("~/") {
            let home = env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
            if input == "~" {
                return Ok(home);
            }
            return Ok(home.join(&input[2..]));
        }

        let path = PathBuf::from(input);
        if path.is_absolute() {
            Ok(path)
        } else {
            Ok(self.root.join(path))
        }
    }

    fn expand_report_path(&self, value: &str) -> Option<PathBuf> {
        if value == "~" || value.starts_with("~/") {
            let home = env::var_os("HOME").map(PathBuf::from)?;
            if value == "~" {
                return Some(home);
            }
            return Some(home.join(&value[2..]));
        }

        Some(PathBuf::from(value))
    }

    fn move_candidate_selection(&mut self, delta: isize) {
        let len = self.visible_indices().len();
        move_table_selection(&mut self.table_state, len, delta);
    }

    fn move_browse_selection(&mut self, delta: isize) {
        let len = self.visible_browse_indices().len();
        move_table_selection(&mut self.browse_state, len, delta);
    }

    fn select_candidate_first(&mut self) {
        let len = self.visible_indices().len();
        select_first(&mut self.table_state, len);
    }

    fn select_candidate_last(&mut self) {
        let len = self.visible_indices().len();
        select_last(&mut self.table_state, len);
    }

    fn select_browse_first(&mut self) {
        let len = self.visible_browse_indices().len();
        select_first(&mut self.browse_state, len);
    }

    fn select_browse_last(&mut self) {
        let len = self.visible_browse_indices().len();
        select_last(&mut self.browse_state, len);
    }

    fn clamp_active_selection(&mut self) {
        match self.view {
            View::Candidates => self.clamp_candidate_selection(),
            View::Browse => self.clamp_browse_selection(),
            View::Growth | View::History => {}
        }
    }

    fn clamp_candidate_selection(&mut self) {
        let len = self.visible_indices().len();
        clamp_table_selection(&mut self.table_state, len);
    }

    fn clamp_browse_selection(&mut self) {
        let len = self.visible_browse_indices().len();
        clamp_table_selection(&mut self.browse_state, len);
    }
}

fn build_clean_plan(
    report: &Report,
    selected_paths: &BTreeSet<String>,
    current_path: Option<&str>,
) -> Option<CleanPlan> {
    let requested: Vec<_> = if selected_paths.is_empty() {
        current_path
            .and_then(|path| {
                report
                    .candidates
                    .iter()
                    .find(|candidate| candidate.path == path)
            })
            .cloned()
            .into_iter()
            .collect()
    } else {
        report
            .candidates
            .iter()
            .filter(|candidate| selected_paths.contains(&candidate.path))
            .cloned()
            .collect()
    };

    if requested.is_empty() {
        return None;
    }

    let requested_count = requested.len();
    let mut included = Vec::new();
    let mut excluded = Vec::new();

    for candidate in requested {
        if candidate.decision == "safe" {
            included.push(candidate);
        } else {
            excluded.push(candidate);
        }
    }

    let allocated_bytes_estimate = included.iter().fold(0_u64, |total, candidate| {
        total.saturating_add(candidate.allocated_bytes_estimate)
    });

    Some(CleanPlan {
        requested_count,
        included,
        excluded,
        allocated_bytes_estimate,
    })
}

fn candidate_matches(candidate: &ReportCandidate, filter: CandidateFilter, query: &str) -> bool {
    let decision_matches = match filter {
        CandidateFilter::All => true,
        CandidateFilter::Safe => candidate.decision == "safe",
        CandidateFilter::Review => candidate.decision == "review",
        CandidateFilter::Protected => candidate.decision == "protected",
    };

    if !decision_matches {
        return false;
    }

    if query.is_empty() {
        return true;
    }

    candidate.path.to_lowercase().contains(query)
        || candidate.kind.to_lowercase().contains(query)
        || candidate.recovery.kind.to_lowercase().contains(query)
        || candidate
            .recovery
            .command
            .as_deref()
            .is_some_and(|command| command.to_lowercase().contains(query))
        || candidate.evidence.iter().any(|evidence| {
            evidence.code.to_lowercase().contains(query)
                || evidence.detail.to_lowercase().contains(query)
        })
}

fn compare_candidates(left: &ReportCandidate, right: &ReportCandidate, sort: SortKey) -> Ordering {
    match sort {
        SortKey::Size => right
            .allocated_bytes_estimate
            .cmp(&left.allocated_bytes_estimate)
            .then_with(|| left.path.cmp(&right.path)),
        SortKey::Decision => decision_rank(&left.decision)
            .cmp(&decision_rank(&right.decision))
            .then_with(|| {
                right
                    .allocated_bytes_estimate
                    .cmp(&left.allocated_bytes_estimate)
            }),
        SortKey::Kind => left.kind.cmp(&right.kind).then_with(|| {
            right
                .allocated_bytes_estimate
                .cmp(&left.allocated_bytes_estimate)
        }),
        SortKey::Path => left.path.cmp(&right.path),
    }
}

fn decision_rank(decision: &str) -> u8 {
    match decision {
        "safe" => 0,
        "review" => 1,
        "protected" => 2,
        _ => 3,
    }
}

fn move_table_selection(state: &mut TableState, len: usize, delta: isize) {
    if len == 0 {
        state.select(None);
        return;
    }

    let current = state.selected().unwrap_or(0);
    let next = if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as usize).min(len - 1)
    };
    state.select(Some(next));
}

fn select_first(state: &mut TableState, len: usize) {
    state.select((len > 0).then_some(0));
}

fn select_last(state: &mut TableState, len: usize) {
    state.select(len.checked_sub(1));
}

fn clamp_table_selection(state: &mut TableState, len: usize) {
    let selected = state.selected().unwrap_or(0);
    state.select(if len == 0 {
        None
    } else {
        Some(selected.min(len - 1))
    });
}

#[cfg(test)]
mod tests {
    use sweep_report::{REPORT_SCHEMA_VERSION, ReportEvidence, ReportRecovery, ReportSummary};

    use super::*;

    fn candidate(path: &str, decision: &str, bytes: u64) -> ReportCandidate {
        ReportCandidate {
            path: path.to_owned(),
            kind: String::from("cargo-target"),
            decision: decision.to_owned(),
            logical_bytes: bytes,
            allocated_bytes_estimate: bytes,
            traversal_complete: true,
            subtree_metadata_fingerprint: None,
            recovery: ReportRecovery {
                kind: String::from("rebuild"),
                command: Some(String::from("cargo build")),
                detail: String::from("rebuildable"),
            },
            evidence: vec![ReportEvidence {
                code: String::from("git_ignored"),
                status: String::from("proven"),
                detail: String::from("ignored by Git"),
            }],
        }
    }

    fn report(candidates: Vec<ReportCandidate>) -> Report {
        let safe_count = candidates
            .iter()
            .filter(|candidate| candidate.decision == "safe")
            .count();
        let review_count = candidates
            .iter()
            .filter(|candidate| candidate.decision == "review")
            .count();
        let protected_count = candidates
            .iter()
            .filter(|candidate| candidate.decision == "protected")
            .count();
        let logical_bytes = candidates
            .iter()
            .map(|candidate| candidate.logical_bytes)
            .sum();
        let allocated_bytes_estimate = candidates
            .iter()
            .map(|candidate| candidate.allocated_bytes_estimate)
            .sum();
        let safe_allocated_bytes_estimate = candidates
            .iter()
            .filter(|candidate| candidate.decision == "safe")
            .map(|candidate| candidate.allocated_bytes_estimate)
            .sum();

        Report {
            schema_version: REPORT_SCHEMA_VERSION,
            created_unix_seconds: 1,
            root: String::from("~/Developer"),
            summary: ReportSummary {
                candidate_count: candidates.len(),
                safe_count,
                review_count,
                protected_count,
                logical_bytes,
                allocated_bytes_estimate,
                safe_allocated_bytes_estimate,
            },
            candidates,
        }
    }

    #[test]
    fn filter_and_search_are_composed() {
        let safe = candidate("~/Developer/a/target", "safe", 10);
        let review = candidate("~/Developer/b/target", "review", 20);

        assert!(candidate_matches(&safe, CandidateFilter::Safe, "cargo"));
        assert!(!candidate_matches(&review, CandidateFilter::Safe, "cargo"));
        assert!(candidate_matches(
            &safe,
            CandidateFilter::All,
            "git_ignored"
        ));
        assert!(!candidate_matches(
            &safe,
            CandidateFilter::All,
            "node_modules"
        ));
    }

    #[test]
    fn size_sort_is_descending() {
        let small = candidate("small", "safe", 10);
        let large = candidate("large", "safe", 20);

        assert_eq!(
            compare_candidates(&small, &large, SortKey::Size),
            Ordering::Greater
        );
    }

    #[test]
    fn clean_preview_includes_only_safe_candidates() {
        let data = report(vec![
            candidate("~/Developer/a/target", "safe", 10),
            candidate("~/Developer/b/target", "review", 20),
            candidate("~/Developer/c/target", "protected", 30),
        ]);
        let selected = data
            .candidates
            .iter()
            .map(|candidate| candidate.path.clone())
            .collect();

        let plan = build_clean_plan(&data, &selected, None).unwrap();

        assert_eq!(plan.requested_count, 3);
        assert_eq!(plan.included.len(), 1);
        assert_eq!(plan.excluded.len(), 2);
        assert_eq!(plan.allocated_bytes_estimate, 10);
        assert_eq!(plan.included[0].decision, "safe");
    }

    #[test]
    fn single_current_candidate_builds_preview_without_marking() {
        let data = report(vec![candidate("~/Developer/a/target", "safe", 10)]);
        let selected = BTreeSet::new();

        let plan = build_clean_plan(&data, &selected, Some("~/Developer/a/target")).unwrap();

        assert_eq!(plan.requested_count, 1);
        assert_eq!(plan.included.len(), 1);
    }
}
