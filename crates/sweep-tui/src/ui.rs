use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
};
use sweep_report::{DeltaDirection, Report, ReportCandidate, candidate_kind_name, decision_name};

use crate::{
    app::{App, CommandFamily, Drawer, InputMode, Overlay, View},
    theme::Theme,
};

const MIN_WIDTH: u16 = 74;
const MIN_HEIGHT: u16 = 18;
const INSPECTOR_WIDTH: u16 = 48;
const INSPECTOR_BREAKPOINT: u16 = 124;
const DRAWER_WIDTH: u16 = 54;

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
    render_body(frame, chunks[2], app, theme);
    render_footer(frame, chunks[3], app, theme);

    if app.view == View::Candidates
        && app.drawer.is_some()
        && chunks[2].width < INSPECTOR_BREAKPOINT
    {
        render_drawer_overlay(frame, area, app, theme);
    }

    match app.overlay {
        Some(Overlay::Help) => render_help(frame, area, app, theme),
        Some(Overlay::Inspect) => render_inspect_overlay(frame, area, app, theme),
        Some(Overlay::Palette) => render_palette(frame, area, app, theme),
        Some(Overlay::PathInput) => render_path_input(frame, area, app, theme),
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
            format!("  {} ", app.view.label()),
            Style::default()
                .fg(theme.bright_text())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "developer storage graph",
            Style::default().fg(theme.muted()),
        ),
    ]);

    let right = Span::styled(status, Style::default().fg(theme.muted()));
    let width = area.width as usize;
    let left_width = 44usize;
    let spacer = width.saturating_sub(left_width + right.content.len());
    let first = Line::from(vec![
        title.spans[0].clone(),
        title.spans[1].clone(),
        title.spans[2].clone(),
        Span::raw(" ".repeat(spacer)),
        right,
    ]);

    let mut scope_spans = vec![
        Span::styled(" scope ", Style::default().fg(theme.muted())),
        Span::styled(app.root_label(), Style::default().fg(theme.text())),
    ];

    if app.view == View::Candidates {
        scope_spans.extend([
            Span::styled("   filter ", Style::default().fg(theme.muted())),
            Span::styled(app.filter.label(), Style::default().fg(theme.accent())),
            Span::styled("   sort ", Style::default().fg(theme.muted())),
            Span::styled(app.sort.label(), Style::default().fg(theme.accent())),
        ]);
    }

    if let Some(message) = &app.status_message {
        scope_spans.extend([
            Span::styled("   ·   ", Style::default().fg(theme.border())),
            Span::styled(compact(message, 68), Style::default().fg(theme.muted())),
        ]);
    }

    frame.render_widget(
        Paragraph::new(vec![first, Line::from(scope_spans)])
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

fn render_body(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: Theme) {
    match app.view {
        View::Candidates => {
            if app.report.is_none() {
                render_loading(frame, area, app, theme);
            } else {
                render_candidates(frame, area, app, theme);
            }
        }
        View::Browse => render_browser(frame, area, app, theme),
        View::Growth => render_growth(frame, area, app, theme),
        View::History => render_history(frame, area, app, theme),
    }
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

fn render_candidates(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: Theme) {
    if area.width >= INSPECTOR_BREAKPOINT {
        let side_width = if app.drawer.is_some() {
            DRAWER_WIDTH
        } else {
            INSPECTOR_WIDTH
        };
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(60), Constraint::Length(side_width)])
            .split(area);

        render_candidate_table(frame, body[0], app, theme);
        if let Some(drawer) = app.drawer.as_ref() {
            render_drawer(frame, body[1], drawer, theme);
        } else {
            render_inspector(frame, body[1], app.selected_candidate(), theme);
        }
    } else {
        render_candidate_table(frame, area, app, theme);
    }
}

fn render_candidate_table(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: Theme) {
    let Some(report) = app.report.as_ref() else {
        return;
    };
    let indices = app.visible_indices();
    let selected_paths = &app.selected_paths;

    let rows: Vec<_> = indices
        .iter()
        .map(|index| {
            let candidate = &report.candidates[*index];
            let mark = if selected_paths.contains(&candidate.path) {
                if app.ascii { "*" } else { "●" }
            } else {
                " "
            };
            Row::new(vec![
                Cell::from(mark),
                Cell::from(format_bytes(candidate.allocated_bytes_estimate)),
                Cell::from(candidate.decision.clone()).style(theme.decision(&candidate.decision)),
                Cell::from(candidate.kind.clone()),
                Cell::from(recovery_label(candidate)),
                Cell::from(candidate.path.clone()),
            ])
            .style(Style::default().fg(theme.text()).bg(theme.background()))
        })
        .collect();

    let header = Row::new(["", "ALLOCATED", "DECISION", "TYPE", "RECOVERY", "PATH"])
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
    let marked = app.selected_paths.len();

    let title = if app.query.is_empty() {
        if marked == 0 {
            format!(" candidates  {} · {} ", indices.len(), visible_allocated)
        } else {
            format!(
                " candidates  {} · {} · {} selected ",
                indices.len(),
                visible_allocated,
                marked
            )
        }
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
            Constraint::Length(2),
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

fn render_browser(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: Theme) {
    if let Some(error) = &app.browse_error {
        render_centered_message(
            frame,
            area,
            "Browse unavailable",
            error,
            theme.protected(),
            theme,
        );
        return;
    }

    let indices = app.visible_browse_indices();
    let rows: Vec<_> = indices
        .iter()
        .map(|index| {
            let entry = &app.browse_entries[*index];
            let kind = if entry.is_symlink {
                "symlink"
            } else if entry.is_dir {
                "dir"
            } else {
                "file"
            };
            let decision = app.candidate_decision_for_path(&entry.path).unwrap_or("—");
            let size = entry
                .bytes
                .map(format_bytes)
                .unwrap_or_else(|| String::from("—"));

            Row::new(vec![
                Cell::from(kind),
                Cell::from(decision.to_owned()).style(theme.decision(decision)),
                Cell::from(size),
                Cell::from(entry.name.clone()),
            ])
            .style(Style::default().fg(theme.text()).bg(theme.background()))
        })
        .collect();

    let header = Row::new(["TYPE", "SWEEP", "SIZE", "NAME"])
        .style(
            Style::default()
                .fg(theme.muted())
                .bg(theme.surface())
                .add_modifier(Modifier::BOLD),
        )
        .bottom_margin(1);

    let title = if app.query.is_empty() {
        format!(
            " browse  {} · {} entries ",
            app.browse_path_label(),
            indices.len()
        )
    } else {
        format!(
            " browse  {} · {} entries  /{} ",
            app.browse_path_label(),
            indices.len(),
            app.query
        )
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(11),
            Constraint::Length(12),
            Constraint::Min(24),
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

    frame.render_stateful_widget(table, area, &mut app.browse_state);
}

fn render_growth(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let Some(diff) = app.growth_diff() else {
        render_centered_message(
            frame,
            area,
            "Snapshot growth",
            &app.growth.message,
            theme.accent(),
            theme,
        );
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(5)])
        .split(area);

    let delta = format_delta(
        diff.summary.allocated_bytes_estimate_delta.direction,
        diff.summary.allocated_bytes_estimate_delta.bytes,
    );
    let summary = vec![
        Line::from(vec![
            Span::styled("LATEST DIFF  ", Style::default().fg(theme.muted())),
            Span::styled(
                format!(
                    "{} -> {}  {delta}",
                    format_bytes(diff.summary.before_allocated_bytes_estimate),
                    format_bytes(diff.summary.after_allocated_bytes_estimate)
                ),
                Style::default()
                    .fg(theme.bright_text())
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!(
                    "{} snapshots · {} invalid ignored · ",
                    app.growth.snapshot_count, app.growth.invalid_snapshot_count
                ),
                Style::default().fg(theme.muted()),
            ),
            Span::styled(
                if diff.complete { "complete" } else { "partial" },
                Style::default().fg(if diff.complete {
                    theme.safe()
                } else {
                    theme.review()
                }),
            ),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(summary)
            .block(
                Block::default()
                    .title(" growth ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.border())),
            )
            .style(theme.panel()),
        chunks[0],
    );

    let mut rows = Vec::new();

    for candidate in &diff.changed {
        let direction = candidate.allocated_bytes_estimate_delta.direction;
        let delta = format_delta(direction, candidate.allocated_bytes_estimate_delta.bytes);
        rows.push(
            Row::new(vec![
                Cell::from(delta).style(Style::default().fg(delta_color(direction, theme))),
                Cell::from(decision_name(candidate.after_decision)),
                Cell::from(candidate_kind_name(candidate.kind)),
                Cell::from(candidate.relative_path.clone()),
            ])
            .style(Style::default().fg(theme.text()).bg(theme.background())),
        );
    }

    for candidate in &diff.added {
        rows.push(
            Row::new(vec![
                Cell::from(format!(
                    "+{}",
                    format_bytes(candidate.allocated_bytes_estimate)
                ))
                .style(Style::default().fg(theme.review())),
                Cell::from(decision_name(candidate.decision)),
                Cell::from(candidate_kind_name(candidate.kind)),
                Cell::from(candidate.relative_path.clone()),
            ])
            .style(Style::default().fg(theme.text()).bg(theme.background())),
        );
    }

    for candidate in &diff.removed {
        rows.push(
            Row::new(vec![
                Cell::from(format!(
                    "-{}",
                    format_bytes(candidate.allocated_bytes_estimate)
                ))
                .style(Style::default().fg(theme.safe())),
                Cell::from(decision_name(candidate.decision)),
                Cell::from(candidate_kind_name(candidate.kind)),
                Cell::from(candidate.relative_path.clone()),
            ])
            .style(Style::default().fg(theme.text()).bg(theme.background())),
        );
    }

    let table = Table::new(
        rows,
        [
            Constraint::Length(13),
            Constraint::Length(11),
            Constraint::Length(18),
            Constraint::Min(24),
        ],
    )
    .header(
        Row::new(["DELTA", "DECISION", "TYPE", "PATH"])
            .style(
                Style::default()
                    .fg(theme.muted())
                    .bg(theme.surface())
                    .add_modifier(Modifier::BOLD),
            )
            .bottom_margin(1),
    )
    .column_spacing(1)
    .block(
        Block::default()
            .title(format!(
                " changes  {} changed · {} new · {} removed · {} moved ",
                diff.summary.changed_count,
                diff.summary.added_count,
                diff.summary.removed_count,
                diff.summary.moved_count
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border())),
    );

    frame.render_widget(table, chunks[1]);
}

fn render_history(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let inner = centered(area, 72, 13);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "No cleanup receipts yet",
            Style::default()
                .fg(theme.bright_text())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Sweep is still in preview mode.",
            Style::default().fg(theme.muted()),
        )),
        Line::from(Span::styled(
            "Preview clean plans never mutate files and never write cleanup history.",
            Style::default().fg(theme.muted()),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("future receipt store  ", Style::default().fg(theme.muted())),
            Span::styled(app.history_path_label(), Style::default().fg(theme.text())),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Once mutation exists, receipts will record plan, revalidation, result and measured reclaim.",
            Style::default().fg(theme.accent()),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" cleanup history ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.border()))
                    .style(theme.panel()),
            ),
        inner,
    );
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

fn render_drawer(frame: &mut Frame<'_>, area: Rect, drawer: &Drawer, theme: Theme) {
    let (title, lines) = match drawer {
        Drawer::CleanPlan(plan) => (
            " clean preview ",
            clean_plan_lines(plan, theme, area.height.saturating_sub(2) as usize),
        ),
        Drawer::PreviewReceipt(plan) => (" preview receipt ", preview_receipt_lines(plan, theme)),
    };

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.accent()))
                    .style(theme.panel()),
            ),
        area,
    );
}

fn clean_plan_lines(
    plan: &crate::app::CleanPlan,
    theme: Theme,
    max_lines: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "PREVIEW ONLY",
            Style::default()
                .fg(theme.review())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "No filesystem mutation is enabled.",
            Style::default().fg(theme.muted()),
        )),
        Line::from(""),
        field(
            "requested",
            &plan.requested_count.to_string(),
            Style::default().fg(theme.text()),
            theme,
        ),
        field(
            "eligible",
            &plan.included.len().to_string(),
            Style::default().fg(theme.safe()),
            theme,
        ),
        field(
            "excluded",
            &plan.excluded.len().to_string(),
            Style::default().fg(if plan.excluded.is_empty() {
                theme.muted()
            } else {
                theme.review()
            }),
            theme,
        ),
        field(
            "estimate",
            &format!("{} allocated", format_bytes(plan.allocated_bytes_estimate)),
            Style::default()
                .fg(theme.bright_text())
                .add_modifier(Modifier::BOLD),
            theme,
        ),
        Line::from(""),
        section("ELIGIBLE SAFE", theme),
    ];

    for candidate in plan.included.iter().take(8) {
        lines.push(Line::from(vec![
            Span::styled("+ ", Style::default().fg(theme.safe())),
            Span::styled(
                format!("{:>9}  ", format_bytes(candidate.allocated_bytes_estimate)),
                Style::default().fg(theme.text()),
            ),
            Span::styled(
                compact(&candidate.path, 34),
                Style::default().fg(theme.muted()),
            ),
        ]));
    }

    if plan.included.len() > 8 {
        lines.push(Line::from(Span::styled(
            format!("  … {} more safe candidate(s)", plan.included.len() - 8),
            Style::default().fg(theme.muted()),
        )));
    }

    if !plan.excluded.is_empty() {
        lines.push(Line::from(""));
        lines.push(section("EXCLUDED", theme));
        for candidate in plan.excluded.iter().take(5) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<10} ", candidate.decision),
                    theme.decision(&candidate.decision),
                ),
                Span::styled(
                    compact(&candidate.path, 34),
                    Style::default().fg(theme.muted()),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "enter simulate plan   esc cancel",
        Style::default().fg(theme.accent()),
    )));

    lines.truncate(max_lines);
    lines
}

fn preview_receipt_lines(plan: &crate::app::CleanPlan, theme: Theme) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "NO FILES CHANGED",
            Style::default()
                .fg(theme.safe())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "The clean flow reached its preview boundary.",
            Style::default().fg(theme.text()),
        )),
        Line::from(Span::styled(
            "Mutation, revalidation and measured reclamation are not enabled yet.",
            Style::default().fg(theme.muted()),
        )),
        Line::from(""),
        field(
            "safe items",
            &plan.included.len().to_string(),
            Style::default().fg(theme.safe()),
            theme,
        ),
        field(
            "excluded",
            &plan.excluded.len().to_string(),
            Style::default().fg(theme.review()),
            theme,
        ),
        field(
            "would target",
            &format!("{} estimate", format_bytes(plan.allocated_bytes_estimate)),
            Style::default().fg(theme.bright_text()),
            theme,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "No cleanup-history record was written.",
            Style::default().fg(theme.muted()),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "enter / esc close",
            Style::default().fg(theme.accent()),
        )),
    ]
}

fn render_drawer_overlay(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let Some(drawer) = app.drawer.as_ref() else {
        return;
    };
    let popup = centered(area, 76, 30);
    frame.render_widget(Clear, popup);
    render_drawer(frame, popup, drawer, theme);
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
    } else if let Some(family) = app.pending_family {
        let mut spans = vec![Span::styled(
            format!(" {} ", family.label()),
            Style::default()
                .fg(theme.background())
                .bg(theme.accent())
                .add_modifier(Modifier::BOLD),
        )];
        match family {
            CommandFamily::Yank => {
                spans.extend(key("y", "text", theme));
                spans.extend(key("p", "path", theme));
                spans.extend(key("m", "markdown", theme));
                spans.extend(key("j", "json", theme));
                spans.extend(key("t", "toml", theme));
            }
            CommandFamily::Save => {
                spans.extend(key("y", "text", theme));
                spans.extend(key("m", "markdown", theme));
                spans.extend(key("j", "json", theme));
                spans.extend(key("t", "toml", theme));
            }
        }
        spans.extend(key("esc", "cancel", theme));
        Line::from(spans)
    } else {
        let mut spans = Vec::new();
        match app.view {
            View::Candidates => {
                spans.extend(key("j/k", "navigate", theme));
                spans.extend(key("space", "select", theme));
                spans.extend(key("c", "clean preview", theme));
                spans.extend(key("o", "Finder", theme));
                spans.extend(key("/", "search", theme));
            }
            View::Browse => {
                spans.extend(key("h/l", "parent/enter", theme));
                spans.extend(key("j/k", "navigate", theme));
                spans.extend(key("o", "Finder", theme));
                spans.extend(key("/", "search", theme));
            }
            View::Growth => {
                spans.extend(key("R", "reload snapshots", theme));
            }
            View::History => {}
        }
        spans.extend(key("y/s", "copy/save", theme));
        spans.extend(key("1-4", "views", theme));
        spans.extend(key(":", "actions", theme));
        spans.extend(key("?", "help", theme));
        spans.extend(key("q", "quit", theme));
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
        Span::styled(format!(" {label}  "), Style::default().fg(theme.muted())),
    ]
}

fn render_help(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let popup = centered(area, 78, 30);
    frame.render_widget(Clear, popup);

    let marker = if app.ascii { ">" } else { "•" };
    let lines = vec![
        section("VIEWS", theme),
        Line::from(format!(
            "{marker} 1 candidates   2 browse   3 growth   4 history"
        )),
        Line::from(format!("{marker} tab                 next view")),
        Line::from(""),
        section("CANDIDATES", theme),
        Line::from(format!("{marker} j / k / arrows      move selection")),
        Line::from(format!("{marker} space               mark candidate")),
        Line::from(format!(
            "{marker} c                   open clean-plan preview"
        )),
        Line::from(format!("{marker} e / enter           inspect evidence")),
        Line::from(format!("{marker} j/k inside inspect  scroll evidence")),
        Line::from(format!("{marker} f / S               filter / sort")),
        Line::from(""),
        section("BROWSE", theme),
        Line::from(format!("{marker} h / left            parent directory")),
        Line::from(format!("{marker} l / right / enter   enter directory")),
        Line::from(format!("{marker} symlink directories are never traversed")),
        Line::from(""),
        section("GLOBAL", theme),
        Line::from(format!(
            "{marker} o                   reveal selected path in Finder"
        )),
        Line::from(format!("{marker} /                   live search")),
        Line::from(format!(
            "{marker} :                   searchable action palette"
        )),
        Line::from(format!("{marker} y y/p/m/j/t         copy report/path")),
        Line::from(format!("{marker} s y/m/j/t           save report")),
        Line::from(format!("{marker} option-left/right   scope history")),
        Line::from(format!(
            "{marker} R                   rescan / reload growth"
        )),
        Line::from(""),
        section("SAFETY", theme),
        Line::from("Clean is preview-only. It cannot mutate files or write cleanup history."),
        Line::from("Browse is read-only and never exposes arbitrary delete."),
        Line::from("Reports and snapshots remain descriptive; they never authorize mutation."),
        Line::from(""),
        Line::from(Span::styled(
            "Esc / Enter closes this panel.",
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

fn render_palette(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let popup = centered(area, 76, 24);
    frame.render_widget(Clear, popup);

    let actions = app.visible_palette_actions();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(" : ", Style::default().fg(theme.accent())),
            Span::styled(
                if app.palette_query.is_empty() {
                    String::from("type to filter actions")
                } else {
                    app.palette_query.clone()
                },
                Style::default().fg(if app.palette_query.is_empty() {
                    theme.muted()
                } else {
                    theme.text()
                }),
            ),
        ]),
        Line::from(""),
    ];

    if actions.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no matching actions",
            Style::default().fg(theme.muted()),
        )));
    } else {
        for (index, action) in actions.iter().enumerate() {
            let selected = index == app.palette_index;
            let prefix = if selected {
                if app.ascii { "> " } else { "▌ " }
            } else {
                "  "
            };
            let style = if selected {
                theme.selected()
            } else {
                Style::default().fg(theme.text())
            };

            lines.push(Line::from(Span::styled(
                format!("{prefix}{}", action.label()),
                style,
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "↑/↓ choose   type filter   backspace edit   enter run   esc close",
        Style::default().fg(theme.muted()),
    )));

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" actions ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.accent()))
                .style(theme.panel()),
        ),
        popup,
    );
}

fn render_path_input(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let popup = centered(area, 76, 9);
    frame.render_widget(Clear, popup);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "New scan scope",
            Style::default()
                .fg(theme.bright_text())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default().fg(theme.muted())),
            Span::styled(
                app.path_input.clone(),
                Style::default().fg(theme.text()).bg(theme.surface_high()),
            ),
            Span::styled("█", Style::default().fg(theme.accent())),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Enter applies · Esc cancels · relative paths resolve from current scope",
            Style::default().fg(theme.muted()),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" change scope ")
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

    let popup = centered(area, 88, 30);
    frame.render_widget(Clear, popup);

    let lines = candidate_lines(candidate, theme, true);
    let visible_lines = popup.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(visible_lines) as u16;
    let scroll = app.inspect_scroll.min(max_scroll);

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((scroll, 0))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(format!(" inspect  {}  ·  j/k scroll ", candidate.kind))
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
        evidence_summary(candidate, theme),
        Line::from(""),
        section("RECOVERY", theme),
        Line::from(candidate.recovery.detail.clone()),
    ];

    if let Some(command) = &candidate.recovery.command {
        lines.push(Line::from(vec![
            Span::styled("$ ", Style::default().fg(theme.safe())),
            Span::styled(command.clone(), Style::default().fg(theme.text())),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(section("EVIDENCE", theme));

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

fn evidence_summary(candidate: &ReportCandidate, theme: Theme) -> Line<'static> {
    let proven = candidate
        .evidence
        .iter()
        .filter(|evidence| evidence.status == "proven")
        .count();
    let refuted = candidate
        .evidence
        .iter()
        .filter(|evidence| evidence.status == "refuted")
        .count();
    let unknown = candidate.evidence.len().saturating_sub(proven + refuted);
    let color = if refuted > 0 {
        theme.protected()
    } else if unknown > 0 {
        theme.review()
    } else {
        theme.safe()
    };

    field(
        "evidence",
        &format!("{proven} proven · {unknown} unknown · {refuted} refuted"),
        Style::default().fg(color),
        theme,
    )
}

fn section(label: &str, theme: Theme) -> Line<'static> {
    Line::from(Span::styled(
        label.to_owned(),
        Style::default()
            .fg(theme.accent())
            .add_modifier(Modifier::BOLD),
    ))
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

fn render_centered_message(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    message: &str,
    title_color: ratatui::style::Color,
    theme: Theme,
) {
    let inner = centered(area, 68, 11);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            title.to_owned(),
            Style::default()
                .fg(title_color)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            message.to_owned(),
            Style::default().fg(theme.muted()),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.border()))
                    .style(theme.panel()),
            ),
        inner,
    );
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

fn delta_color(direction: DeltaDirection, theme: Theme) -> ratatui::style::Color {
    match direction {
        DeltaDirection::Increased => theme.review(),
        DeltaDirection::Decreased => theme.safe(),
        DeltaDirection::Unchanged => theme.muted(),
    }
}

fn format_delta(direction: DeltaDirection, bytes: u64) -> String {
    match direction {
        DeltaDirection::Increased => format!("+{}", format_bytes(bytes)),
        DeltaDirection::Decreased => format!("-{}", format_bytes(bytes)),
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
