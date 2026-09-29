use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
};
use sweep_report::{Report, ReportCandidate};

use crate::{
    app::{App, InputMode, Overlay},
    theme::Theme,
};

const MIN_WIDTH: u16 = 74;
const MIN_HEIGHT: u16 = 18;
const INSPECTOR_WIDTH: u16 = 48;
const INSPECTOR_BREAKPOINT: u16 = 124;

pub(crate) fn render(frame: &mut Frame<'_>, app: &mut App, frame_delta: Duration) {
    let area = frame.area();
    let theme = Theme::new(app.no_color);
    frame.render_widget(Block::default().style(theme.base()), area);

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_too_small(frame, area, theme);
        process_effects(frame, app, frame_delta);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(7),
            Constraint::Length(2),
        ])
        .split(area);

    render_header(frame, chunks[0], app, theme);
    render_summary(frame, chunks[1], app.report.as_ref(), theme);

    if app.report.is_none() {
        render_loading(frame, chunks[2], app, theme);
    } else {
        render_body(frame, chunks[2], app, theme);
    }

    render_footer(frame, chunks[3], app, theme);

    match app.overlay {
        Some(Overlay::Help) => render_help(frame, area, app, theme),
        Some(Overlay::Inspect) => render_inspect_overlay(frame, area, app, theme),
        None => {}
    }

    process_effects(frame, app, frame_delta);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let status = if app.scanning {
        format!("SCANNING  {:>5.1}s", app.scan_elapsed.as_secs_f64())
    } else if let Some(error) = &app.last_error {
        format!("ERROR  {}", compact(error, 32))
    } else if app.discovery_error_count > 0 {
        format!("READY  PARTIAL ({})", app.discovery_error_count)
    } else {
        String::from("READY  COMPLETE")
    };

    let title = Line::from(vec![
        Span::styled(
            " SWEEP ",
            Style::default()
                .fg(theme.background())
                .bg(theme.accent())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  developer storage graph",
            Style::default().fg(theme.muted()),
        ),
    ]);

    let right = Span::styled(status, Style::default().fg(theme.muted()));
    let width = area.width as usize;
    let left_width = 34usize;
    let spacer = width.saturating_sub(left_width + right.content.len());
    let first = Line::from(vec![
        title.spans[0].clone(),
        title.spans[1].clone(),
        Span::raw(" ".repeat(spacer)),
        right,
    ]);

    let scope = Line::from(vec![
        Span::styled(" scope ", Style::default().fg(theme.muted())),
        Span::styled(app.root_label(), Style::default().fg(theme.text())),
        Span::styled("   filter ", Style::default().fg(theme.muted())),
        Span::styled(app.filter.label(), Style::default().fg(theme.accent())),
        Span::styled("   sort ", Style::default().fg(theme.muted())),
        Span::styled(app.sort.label(), Style::default().fg(theme.accent())),
    ]);

    frame.render_widget(
        Paragraph::new(vec![first, scope])
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(theme.border())),
            )
            .style(theme.base()),
        area,
    );
}

fn render_summary(frame: &mut Frame<'_>, area: Rect, report: Option<&Report>, theme: Theme) {
    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    match report {
        Some(report) => {
            metric(
                frame,
                cards[0],
                "OBSERVED",
                &format_bytes(report.summary.allocated_bytes_estimate),
                "allocated estimate",
                theme.text(),
                theme,
            );
            metric(
                frame,
                cards[1],
                "SAFE",
                &format_bytes(report.summary.safe_allocated_bytes_estimate),
                &format!("{} candidates", report.summary.safe_count),
                theme.safe(),
                theme,
            );
            metric(
                frame,
                cards[2],
                "REVIEW",
                &report.summary.review_count.to_string(),
                "evidence incomplete",
                theme.review(),
                theme,
            );
            metric(
                frame,
                cards[3],
                "PROTECTED",
                &report.summary.protected_count.to_string(),
                "never automatic",
                theme.protected(),
                theme,
            );
        }
        None => {
            for (index, label) in ["OBSERVED", "SAFE", "REVIEW", "PROTECTED"]
                .iter()
                .enumerate()
            {
                metric(
                    frame,
                    cards[index],
                    label,
                    "--",
                    "scanning",
                    theme.muted(),
                    theme,
                );
            }
        }
    }
}

fn metric(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    value: &str,
    subtitle: &str,
    value_color: ratatui::style::Color,
    theme: Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border()))
        .style(theme.panel());

    let lines = vec![
        Line::from(Span::styled(label, Style::default().fg(theme.muted()))),
        Line::from(Span::styled(
            value,
            Style::default()
                .fg(value_color)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(subtitle, Style::default().fg(theme.muted()))),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Left),
        area,
    );
}

fn render_loading(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let inner = centered(area, 56, 9);
    let spinner = spinner(app.scan_elapsed, app.ascii);
    let title = if app.last_error.is_some() {
        "Scan unavailable"
    } else {
        "Building evidence graph"
    };

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{spinner} "), Style::default().fg(theme.accent())),
            Span::styled(
                title,
                Style::default()
                    .fg(theme.text())
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            app.root_label(),
            Style::default().fg(theme.muted()),
        )),
        Line::from(""),
    ];

    if let Some(error) = &app.last_error {
        lines.push(Line::from(Span::styled(
            compact(error, 52),
            Style::default().fg(theme.protected()),
        )));
        lines.push(Line::from(Span::styled(
            "Press R to retry.",
            Style::default().fg(theme.muted()),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Discovery -> evidence -> classification -> report",
            Style::default().fg(theme.muted()),
        )));
        lines.push(Line::from(Span::styled(
            "Read-only. No files are modified.",
            Style::default().fg(theme.safe()),
        )));
    }

    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center).block(
            Block::default()
                .title(" live scan ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.accent()))
                .style(theme.panel()),
        ),
        inner,
    );
}

fn render_body(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: Theme) {
    if area.width >= INSPECTOR_BREAKPOINT {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(60), Constraint::Length(INSPECTOR_WIDTH)])
            .split(area);
        render_table(frame, body[0], app, theme);
        render_inspector(frame, body[1], app.selected_candidate(), theme);
    } else {
        render_table(frame, area, app, theme);
    }
}

fn render_table(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: Theme) {
    let Some(report) = app.report.as_ref() else {
        return;
    };
    let indices = app.visible_indices();

    let rows = indices.iter().map(|index| {
        let candidate = &report.candidates[*index];
        Row::new(vec![
            Cell::from(format_bytes(candidate.allocated_bytes_estimate)),
            Cell::from(candidate.decision.clone()).style(theme.decision(&candidate.decision)),
            Cell::from(candidate.kind.clone()),
            Cell::from(recovery_label(candidate)),
            Cell::from(candidate.path.clone()),
        ])
        .style(Style::default().fg(theme.text()).bg(theme.background()))
    });

    let header = Row::new(["ALLOCATED", "DECISION", "TYPE", "RECOVERY", "PATH"])
        .style(
            Style::default()
                .fg(theme.muted())
                .bg(theme.surface())
                .add_modifier(Modifier::BOLD),
        )
        .bottom_margin(1);

    let visible_allocated_bytes = indices
        .iter()
        .map(|index| report.candidates[*index].allocated_bytes_estimate)
        .sum::<u64>();
    let visible_allocated = format_bytes(visible_allocated_bytes);

    let title = if app.query.is_empty() {
        format!(" candidates  {} · {} ", indices.len(), visible_allocated)
    } else {
        format!(
            " candidates  {} · {}  /{} ",
            indices.len(),
            visible_allocated,
            app.query
        )
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(11),
            Constraint::Length(17),
            Constraint::Length(18),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .column_spacing(1)
    .row_highlight_style(theme.selected())
    .highlight_symbol(if app.ascii { "> " } else { "▌ " })
    .block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border())),
    );

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_inspector(
    frame: &mut Frame<'_>,
    area: Rect,
    candidate: Option<&ReportCandidate>,
    theme: Theme,
) {
    let block = Block::default()
        .title(" evidence ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border()))
        .style(theme.panel());

    let Some(candidate) = candidate else {
        frame.render_widget(
            Paragraph::new("No candidate selected.")
                .style(Style::default().fg(theme.muted()))
                .block(block),
            area,
        );
        return;
    };

    let mut lines = candidate_lines(candidate, theme, false);
    let available = area.height.saturating_sub(2) as usize;
    lines.truncate(available);

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let line = if app.input_mode == InputMode::Search {
        Line::from(vec![
            Span::styled(
                " / ",
                Style::default()
                    .fg(theme.background())
                    .bg(theme.accent())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if app.query.is_empty() {
                    "type to search".to_owned()
                } else {
                    app.query.clone()
                },
                Style::default().fg(theme.text()),
            ),
            Span::styled(
                "   enter/esc done   ctrl-u clear",
                Style::default().fg(theme.muted()),
            ),
        ])
    } else {
        let mut spans = Vec::new();
        spans.extend(key("q", "quit", theme));
        spans.extend(key("j/k", "navigate", theme));
        spans.extend(key("/", "search", theme));
        spans.extend(key("f", "filter", theme));
        spans.extend(key("S", "sort", theme));
        spans.extend(key("e", "inspect", theme));
        spans.extend(key("R", "rescan", theme));
        spans.extend(key("?", "help", theme));
        Line::from(spans)
    };

    frame.render_widget(
        Paragraph::new(line).style(theme.base()).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(theme.border())),
        ),
        area,
    );
}

fn key(key: &str, label: &str, theme: Theme) -> [Span<'static>; 2] {
    [
        Span::styled(
            format!(" {key}"),
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {label}  "),
            Style::default().fg(theme.muted()),
        ),
    ]
}

fn render_help(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let popup = centered(area, 70, 22);
    frame.render_widget(Clear, popup);

    let marker = if app.ascii { ">" } else { "•" };
    let lines = vec![
        Line::from(Span::styled(
            "Navigation",
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("{marker} j / k / arrows     move selection")),
        Line::from(format!("{marker} g g / G            first / last")),
        Line::from(format!("{marker} enter / e          inspect evidence")),
        Line::from(""),
        Line::from(Span::styled(
            "View",
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("{marker} /                  live search")),
        Line::from(format!("{marker} f                  cycle decision filter")),
        Line::from(format!("{marker} S                  cycle sort order")),
        Line::from(format!("{marker} R                  rescan current scope")),
        Line::from(""),
        Line::from(Span::styled(
            "Safety",
            Style::default()
                .fg(theme.safe())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("This surface is descriptive only."),
        Line::from("It cannot mutate files or turn a report into deletion authority."),
        Line::from(""),
        Line::from(Span::styled(
            "Accessibility",
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("NO_COLOR=1 disables color. SWEEP_ASCII=1 uses ASCII markers."),
        Line::from(""),
        Line::from(Span::styled(
            "Esc / Enter closes this panel. q quits Sweep.",
            Style::default().fg(theme.muted()),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(" Sweep keymap ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.accent()))
                .style(theme.panel()),
        ),
        popup,
    );
}

fn render_inspect_overlay(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let Some(candidate) = app.selected_candidate() else {
        return;
    };

    let popup = centered(area, 84, 28);
    frame.render_widget(Clear, popup);

    frame.render_widget(
        Paragraph::new(Text::from(candidate_lines(candidate, theme, true)))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(format!(" inspect  {} ", candidate.kind))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.accent()))
                    .style(theme.panel()),
            ),
        popup,
    );
}

fn candidate_lines(
    candidate: &ReportCandidate,
    theme: Theme,
    expanded: bool,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            candidate.path.clone(),
            Style::default()
                .fg(theme.text())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        field(
            "decision",
            &candidate.decision,
            theme.decision(&candidate.decision),
            theme,
        ),
        field(
            "allocated",
            &format!(
                "{} estimate",
                format_bytes(candidate.allocated_bytes_estimate)
            ),
            Style::default().fg(theme.text()),
            theme,
        ),
        field(
            "logical",
            &format_bytes(candidate.logical_bytes),
            Style::default().fg(theme.text()),
            theme,
        ),
        field(
            "traversal",
            if candidate.traversal_complete {
                "complete"
            } else {
                "incomplete"
            },
            Style::default().fg(if candidate.traversal_complete {
                theme.safe()
            } else {
                theme.review()
            }),
            theme,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "RECOVERY",
            Style::default()
                .fg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(candidate.recovery.detail.clone()),
    ];

    if let Some(command) = &candidate.recovery.command {
        lines.push(Line::from(vec![
            Span::styled("$ ", Style::default().fg(theme.safe())),
            Span::styled(command.clone(), Style::default().fg(theme.text())),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "EVIDENCE",
        Style::default()
            .fg(theme.accent())
            .add_modifier(Modifier::BOLD),
    )));

    let evidence_limit = if expanded { usize::MAX } else { 7 };
    for evidence in candidate.evidence.iter().take(evidence_limit) {
        let (symbol, color) = match evidence.status.as_str() {
            "proven" => ("+", theme.safe()),
            "refuted" => ("!", theme.protected()),
            _ => ("?", theme.review()),
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{symbol} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}  ", evidence.code),
                Style::default().fg(theme.text()),
            ),
            Span::styled(evidence.detail.clone(), Style::default().fg(theme.muted())),
        ]));
    }

    if !expanded && candidate.evidence.len() > evidence_limit {
        lines.push(Line::from(Span::styled(
            format!(
                "… {} more — press e",
                candidate.evidence.len() - evidence_limit
            ),
            Style::default().fg(theme.muted()),
        )));
    }

    lines
}

fn field(label: &str, value: &str, value_style: Style, theme: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<12}"), Style::default().fg(theme.muted())),
        Span::styled(value.to_owned(), value_style),
    ])
}

fn recovery_label(candidate: &ReportCandidate) -> String {
    candidate
        .recovery
        .command
        .as_deref()
        .map(|command| compact(command, 17))
        .unwrap_or_else(|| candidate.recovery.kind.clone())
}

fn render_too_small(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let message = Paragraph::new(vec![
        Line::from(Span::styled(
            "Sweep needs a little more room.",
            Style::default()
                .fg(theme.text())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "Current: {}x{}   Minimum: {MIN_WIDTH}x{MIN_HEIGHT}",
                area.width, area.height
            ),
            Style::default().fg(theme.muted()),
        )),
    ])
    .alignment(Alignment::Center)
    .style(theme.base());

    frame.render_widget(message, centered(area, 58, 5));
}

fn process_effects(frame: &mut Frame<'_>, app: &mut App, frame_delta: Duration) {
    if app.no_color {
        return;
    }

    let area = frame.area();
    app.effects
        .process_effects(frame_delta.into(), frame.buffer_mut(), area);
}

fn spinner(elapsed: Duration, ascii: bool) -> &'static str {
    let frame = ((elapsed.as_millis() / 90) % 8) as usize;
    if ascii {
        ["|", "/", "-", "\\", "|", "/", "-", "\\"][frame]
    } else {
        ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"][frame]
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width),
            Constraint::Fill(1),
        ])
        .split(area);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height),
            Constraint::Fill(1),
        ])
        .split(horizontal[1]);
    vertical[1]
}

fn compact(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_owned();
    }
    if max_chars <= 1 {
        return String::from("…");
    }

    let mut compacted: String = value.chars().take(max_chars - 1).collect();
    compacted.push('…');
    compacted
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
