use std::{
    cmp::Ordering,
    env, io,
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
     widgets::TableState,
};
use sweep_report::{Report, ReportCandidate};
use sweep_scan::{ScanOptions, scan_with_diagnostics};
use tachyonfx::{EffectManager, Interpolation, Motion, fx};

use crate::{display_path, theme::Theme, ui};

const ACTIVE_FRAME_TIME: Duration = Duration::from_millis(33);
const IDLE_FRAME_TIME: Duration = Duration::from_millis(120);

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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Overlay {
    Help,
    Inspect,
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
    pub(crate) filter: CandidateFilter,
    pub(crate) sort: SortKey,
    pub(crate) query: String,
    pub(crate) input_mode: InputMode,
    pub(crate) overlay: Option<Overlay>,
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
}

impl App {
    pub(crate) fn new(root: PathBuf) -> io::Result<Self> {
        let root = root.canonicalize()?;
        let no_color = env::var_os("NO_COLOR").is_some()
            || env::var_os("TERM").is_some_and(|value| value == "dumb");
        let ascii = env::var_os("SWEEP_ASCII").is_some();

        let mut app = Self {
            root,
            report: None,
            table_state: TableState::default(),
            filter: CandidateFilter::All,
            sort: SortKey::Size,
            query: String::new(),
            input_mode: InputMode::Normal,
            overlay: None,
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
        };

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

    pub(crate) fn selected_candidate(&self) -> Option<&ReportCandidate> {
        let report = self.report.as_ref()?;
        let indices = self.visible_indices();
        let selected = self.table_state.selected()?;
        let index = *indices.get(selected)?;
        report.candidates.get(index)
    }

    pub(crate) fn root_label(&self) -> String {
        display_path(&self.root)
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
                self.clamp_selection();

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

        if self.input_mode == InputMode::Search {
            self.on_search_key(key);
            return Ok(());
        }

        if let Some(overlay) = self.overlay {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.overlay = None,
                KeyCode::Char('?') if overlay == Overlay::Help => self.overlay = None,
                KeyCode::Char('q') => self.should_quit = true,
                _ => {}
            }
            return Ok(());
        }

        if key.code != KeyCode::Char('g') {
            self.pending_g = false;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.overlay = Some(Overlay::Help),
            KeyCode::Char('e') | KeyCode::Enter => {
                if self.selected_candidate().is_some() {
                    self.overlay = Some(Overlay::Inspect);
                }
            }
            KeyCode::Char('/') => self.input_mode = InputMode::Search,
            KeyCode::Char('f') => {
                self.filter = self.filter.next();
                self.clamp_selection();
            }
            KeyCode::Char('S') => {
                self.sort = self.sort.next();
                self.clamp_selection();
            }
            KeyCode::Char('R') => self.start_scan()?,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Home => self.select_first(),
            KeyCode::End | KeyCode::Char('G') => self.select_last(),
            KeyCode::Char('g') => {
                if self.pending_g {
                    self.select_first();
                    self.pending_g = false;
                } else {
                    self.pending_g = true;
                }
            }
            KeyCode::Esc if !self.query.is_empty() => {
                self.query.clear();
                self.clamp_selection();
            }
            _ => {}
        }

        Ok(())
    }

    fn on_search_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
            self.query.clear();
            self.clamp_selection();
            return;
        }

        match key.code {
            KeyCode::Enter | KeyCode::Esc => self.input_mode = InputMode::Normal,
            KeyCode::Backspace => {
                self.query.pop();
                self.clamp_selection();
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.push(character);
                self.clamp_selection();
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.visible_indices().len();
        if len == 0 {
            self.table_state.select(None);
            return;
        }

        let current = self.table_state.selected().unwrap_or(0);
        let next = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize).min(len - 1)
        };
        self.table_state.select(Some(next));
    }

    fn select_first(&mut self) {
        if self.visible_indices().is_empty() {
            self.table_state.select(None);
        } else {
            self.table_state.select(Some(0));
        }
    }

    fn select_last(&mut self) {
        let len = self.visible_indices().len();
        self.table_state.select(len.checked_sub(1));
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_indices().len();
        let selected = self.table_state.selected().unwrap_or(0);
        self.table_state.select(if len == 0 {
            None
        } else {
            Some(selected.min(len - 1))
        });
    }
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

#[cfg(test)]
mod tests {
    use sweep_report::{ReportEvidence, ReportRecovery};

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
    fn decision_sort_keeps_safe_before_review() {
        let safe = candidate("safe", "safe", 10);
        let review = candidate("review", "review", 20);

        assert_eq!(
            compare_candidates(&safe, &review, SortKey::Decision),
            Ordering::Less
        );
    }
}
