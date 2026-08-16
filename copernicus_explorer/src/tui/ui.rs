use super::app::{
    App, DownloadState, DownloadUiStatus, FilterField, MAX_CONCURRENT_DOWNLOADS, Pane,
    download_rate, format_bytes, format_eta, format_rate,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table};

pub fn draw(frame: &mut Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(9),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_title(frame, root[0]);
    draw_main(frame, app, root[1]);
    draw_downloads(frame, app, root[2]);
    draw_status(frame, app, root[3]);
    draw_help(frame, app, root[4]);
}

fn draw_title(frame: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            " Copernicus Explorer ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" terminal UI"),
    ]));
    frame.render_widget(title, area);
}

fn draw_main(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);

    draw_filters(frame, app, chunks[0]);
    draw_results(frame, app, chunks[1]);
}

fn pane_block<'a>(title: &'a str, focused: bool) -> Block<'a> {
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Span::styled(
            title,
            if focused {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
        ))
}

fn draw_filters(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Pane::Filters;
    let block = pane_block(" Filters ", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();
    for field in FilterField::ALL {
        let selected = focused && app.filter_field == field;
        let editing = selected && app.editing;
        let value = app.field_value(field);
        let marker = if editing {
            ">"
        } else if selected {
            "*"
        } else {
            " "
        };
        let style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let suffix = if editing {
            "█"
        } else if matches!(field, FilterField::Satellite | FilterField::Product) && selected {
            " ←→"
        } else {
            ""
        };
        lines.push(Line::from(Span::styled(
            format!(
                "{marker} {:<24} {value}{suffix}",
                format!("{}:", field.label())
            ),
            style,
        )));
    }

    if app.searching {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Searching…",
            Style::default().fg(Color::Cyan),
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_results(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Pane::Results;
    let downloaded_n = app
        .products
        .iter()
        .filter(|p| app.is_downloaded(&p.id))
        .count();
    let downloading_n = app
        .products
        .iter()
        .filter(|p| app.is_downloading(&p.id))
        .count();
    let title = match (downloaded_n, downloading_n) {
        (0, 0) => format!(" Results ({}) ", app.products.len()),
        (d, 0) => format!(" Results ({}, {d}✓) ", app.products.len()),
        (0, n) => format!(" Results ({}, {n}↓) ", app.products.len()),
        (d, n) => format!(" Results ({}, {d}✓ {n}↓) ", app.products.len()),
    };
    let block = pane_block(&title, focused);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    if app.products.is_empty() {
        let empty = Paragraph::new("No results yet. Adjust filters and press s to search.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, inner);
        return;
    }

    let header = Row::new(["", "Name", "Date", "Cloud", "Online"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(0);

    let rows = app.products.iter().enumerate().map(|(i, p)| {
        let cloud = p
            .cloud_cover
            .map(|c| format!("{c:.0}%"))
            .unwrap_or_else(|| "-".to_string());
        let online = if p.online { "yes" } else { "no" };
        let downloaded = app.is_downloaded(&p.id);
        let downloading = app.is_downloading(&p.id);
        let mark = if downloaded {
            "✓"
        } else if downloading {
            "↓"
        } else if app.marked.contains(&p.id) {
            "•"
        } else {
            " "
        };
        let name = truncate(&p.name, inner.width.saturating_sub(32) as usize);
        let date = if p.acquisition_date.len() >= 10 {
            &p.acquisition_date[..10]
        } else {
            p.acquisition_date.as_str()
        };
        let row = Row::new([
            Cell::from(mark),
            Cell::from(name),
            Cell::from(date.to_string()),
            Cell::from(cloud),
            Cell::from(online),
        ]);
        let blue = Color::Rgb(80, 160, 255);
        if focused && i == app.selected_result {
            let bg = if downloaded {
                Color::Green
            } else if downloading {
                blue
            } else {
                Color::Cyan
            };
            row.style(
                Style::default()
                    .fg(Color::Black)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            )
        } else if downloaded {
            row.style(Style::default().fg(Color::Green))
        } else if downloading {
            row.style(Style::default().fg(blue))
        } else if app.marked.contains(&p.id) {
            row.style(Style::default().fg(Color::Yellow))
        } else {
            row
        }
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(1),
            Constraint::Min(20),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .column_spacing(1);

    frame.render_widget(table, inner);
}

fn draw_downloads(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Pane::Downloads;
    let active = {
        let map = app.downloads.lock().unwrap();
        map.values()
            .filter(|s| matches!(s.status, DownloadUiStatus::Downloading))
            .count()
    };
    let title = if active > 0 {
        format!(" Downloads ({active} active, max {MAX_CONCURRENT_DOWNLOADS}) ")
    } else {
        " Downloads ".to_string()
    };
    let block = pane_block(&title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.download_order.is_empty() {
        let empty =
            Paragraph::new("No downloads yet. Space mark · d download · a download all (async).")
                .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, inner);
        return;
    }

    let map = app.downloads.lock().unwrap();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            app.download_order
                .iter()
                .map(|_| Constraint::Length(1))
                .collect::<Vec<_>>(),
        )
        .split(inner);

    for (i, id) in app.download_order.iter().enumerate() {
        if i >= chunks.len() {
            break;
        }
        let Some(state) = map.get(id) else {
            continue;
        };
        let selected = focused && i == app.selected_download;
        match &state.status {
            DownloadUiStatus::Downloading => {
                frame.render_widget(
                    Paragraph::new(progress_line(state, chunks[i].width as usize, selected)),
                    chunks[i],
                );
            }
            DownloadUiStatus::Completed(path) => {
                let style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Green)
                };
                let label = truncate(&state.label, 40);
                frame.render_widget(
                    Paragraph::new(format!("✓ {label} → {path}")).style(style),
                    chunks[i],
                );
            }
            DownloadUiStatus::Failed(message) => {
                let style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Red)
                };
                let label = truncate(&state.label, 40);
                frame.render_widget(
                    Paragraph::new(format!("✗ {label}: {message}")).style(style),
                    chunks[i],
                );
            }
        }
    }
}

/// Build a CLI-style progress line: `name  [████░░░░]  42%  12 MiB/30 MiB  2 MiB/s  eta 9s`.
fn progress_line(state: &DownloadState, width: usize, selected: bool) -> Line<'static> {
    let rate = download_rate(state.downloaded, state.started_at);
    let (ratio, stats) = if let Some(total) = state.total.filter(|t| *t > 0) {
        let ratio = (state.downloaded as f64 / total as f64).clamp(0.0, 1.0);
        let mut parts = vec![
            format!("{:.0}%", ratio * 100.0),
            format!("{}/{}", format_bytes(state.downloaded), format_bytes(total)),
        ];
        if let Some(r) = rate {
            parts.push(format_rate(r));
            let remaining = (total.saturating_sub(state.downloaded)) as f64 / r;
            parts.push(format!("eta {}", format_eta(remaining)));
        }
        (Some(ratio), parts.join("  "))
    } else {
        let mut parts = vec![format_bytes(state.downloaded)];
        if let Some(r) = rate {
            parts.push(format_rate(r));
        } else {
            parts.push("…".to_string());
        }
        (None, parts.join("  "))
    };

    // Reserve space for "  [bar]  " + stats; give the rest to the label.
    let bar_width = if ratio.is_some() {
        (width / 4).clamp(12, 28)
    } else {
        0
    };
    let chrome = if bar_width > 0 {
        2 + bar_width + 2 + 2 + stats.chars().count()
    } else {
        2 + stats.chars().count()
    };
    let label_budget = width.saturating_sub(chrome).max(8);
    let label = truncate(&state.label, label_budget);

    let bar_style = if selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let dim = Style::default().fg(Color::DarkGray);
    let text_style = if selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let mut spans = vec![Span::styled(label, text_style), Span::raw("  ")];
    if let Some(ratio) = ratio {
        spans.push(Span::styled("[", dim));
        spans.push(Span::styled(progress_bar(ratio, bar_width), bar_style));
        spans.push(Span::styled("]", dim));
        spans.push(Span::raw("  "));
    } else {
        spans.push(Span::styled(spinner_frame(state), bar_style));
        spans.push(Span::raw("  "));
    }
    spans.push(Span::styled(stats, text_style));
    Line::from(spans)
}

fn progress_bar(ratio: f64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    // Match indicatif's `progress_chars("█▓░")`: filled / current / empty.
    let pos = (ratio * width as f64).clamp(0.0, width as f64);
    let filled = pos.floor() as usize;
    let frac = pos - filled as f64;
    let mut out = String::with_capacity(width);
    for i in 0..width {
        if i < filled {
            out.push('█');
        } else if i == filled && frac > 0.0 {
            out.push('▓');
        } else {
            out.push('░');
        }
    }
    out
}

fn spinner_frame(state: &DownloadState) -> &'static str {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let ticks = state
        .started_at
        .map(|t| t.elapsed().as_millis() / 80)
        .unwrap_or(0);
    FRAMES[(ticks as usize) % FRAMES.len()]
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let status =
        Paragraph::new(format!(" {}", app.status)).style(Style::default().fg(Color::White));
    frame.render_widget(status, area);
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let help = if app.editing {
        " editing  Enter/Esc finish  Backspace delete  Ctrl-u clear "
    } else {
        match app.focus {
            Pane::Filters => {
                " Tab/Alt+←→  ↑↓ fields  ←→ sat/product  Enter edit  s search  S append  q "
            }
            Pane::Results => " Tab/Alt+←→  ↑↓/jk  Space  d/a  ↓=busy ✓=done  s/S search  q quit ",
            Pane::Downloads => " Tab/Alt+←→/Esc  ↑↓/jk  d/a queue  s/S search  q quit ",
        }
    };
    let para = Paragraph::new(help).style(
        Style::default()
            .fg(Color::Black)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(para, area);
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else if max <= 1 {
        "…".to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{truncated}…")
    }
}
