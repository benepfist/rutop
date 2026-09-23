use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Sparkline, Table, Wrap};
use ratatui::Frame;

use super::app::{App, Overlay, TextView};
use crate::config::Mode;
use crate::modes::Change;
use crate::text::THREAD_COLS;

const HELP: &[(&str, &str)] = &[
    ("?", "display this screen"),
    ("c", "command summary view (based on Com_* counters)"),
    ("d", "show only a specific database"),
    ("e", "explain the query that a thread is running"),
    ("f", "show full query info for a given thread"),
    ("F", "unFilter the display"),
    ("h", "show only a specific host's connections"),
    ("H", "toggle the header"),
    ("i", "toggle the display of idle (sleeping) threads"),
    ("I", "show innodb status"),
    ("k", "kill a thread"),
    ("K", "kill all threads of a user"),
    ("L", "toggle short/long numbers"),
    ("m", "switch [mode] to qps (queries/sec) scrolling view"),
    ("o", "reverse the sort order (toggle)"),
    ("p", "pause the display"),
    ("q", "quit (in other modes: back to thread view)"),
    ("r", "reset the status counters (via FLUSH STATUS on your server)"),
    ("R", "toggle resolving of IP addresses to hostnames"),
    ("s", "change the delay between screen updates"),
    ("S", "show status counters (SHOW GLOBAL STATUS) with changes"),
    ("t", "switch to thread view (default)"),
    ("u", "show only a specific user"),
    ("V", "show server variables (SHOW VARIABLES)"),
    ("D", "show the current configuration"),
    ("#", "toggle debug info (query errors, refresh time)"),
    ("↑↓ PgUp PgDn", "scroll"),
];

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let header: Vec<String> = if app.mode == Mode::Top && app.mon.cfg.header {
        app.mon.header_lines(area.width as usize)
    } else {
        Vec::new()
    };
    let header_h = if header.is_empty() { 0 } else { header.len() as u16 + 1 };
    let [head, main, bar] = Layout::vertical([
        Constraint::Length(header_h),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    if !header.is_empty() {
        let lines: Vec<Line> = header
            .into_iter()
            .enumerate()
            .map(|(i, l)| {
                if i == 0 {
                    Line::from(l).bold().cyan()
                } else {
                    Line::from(l)
                }
            })
            .collect();
        f.render_widget(Paragraph::new(lines), head);
    }

    match app.mode {
        Mode::Top => draw_threads(f, app, main),
        Mode::Qps => draw_qps(f, app, main),
        Mode::Cmd => draw_cmd(f, app, main),
        Mode::Status => draw_status(f, app, main),
        Mode::Innodb => draw_innodb(f, app, main),
    }

    draw_bar(f, app, bar);

    match &mut app.overlay {
        Overlay::Help => draw_help(f, area),
        Overlay::Text(view) => draw_text(f, view, area),
        _ => {}
    }
}

/// Clamp the main scroll offset and remember the page size.
fn clamp_scroll(app: &mut App, total: usize, visible: usize) -> usize {
    app.page = visible.max(1);
    let max = total.saturating_sub(visible);
    app.scroll = app.scroll.min(max);
    app.scroll
}

fn title_line(title: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(title.to_string(), Style::new().bold()),
        Span::styled("  [q/t: back to thread view]", Style::new().dark_gray()),
    ])
}

fn split_title(area: Rect) -> (Rect, Rect) {
    let [t, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    (t, rest)
}

fn header_style() -> Style {
    Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED)
}

fn right(s: impl Into<String>) -> Cell<'static> {
    Cell::from(Line::from(s.into()).right_aligned())
}

fn draw_threads(f: &mut Frame, app: &mut App, area: Rect) {
    let visible_rows = area.height.saturating_sub(1) as usize;
    let threads = app.mon.visible_threads();
    let total = threads.len();
    let widths: Vec<Constraint> = THREAD_COLS
        .iter()
        .map(|w| Constraint::Length(*w as u16))
        .chain([Constraint::Fill(1)])
        .collect();
    let rows: Vec<Row> = {
        let offset = {
            let max = total.saturating_sub(visible_rows);
            app.scroll.min(max)
        };
        threads
            .iter()
            .skip(offset)
            .take(visible_rows)
            .map(|t| {
                let style = match t.command.as_str() {
                    "Query" => Style::new().yellow(),
                    "Sleep" => Style::new().gray(),
                    "Connect" => Style::new().green(),
                    _ => Style::new(),
                };
                Row::new(vec![
                    right(t.id.to_string()),
                    Cell::from(t.user.clone()),
                    Cell::from(t.host.clone()),
                    Cell::from(t.db.clone()),
                    right(t.time.to_string()),
                    Cell::from(t.command.clone()),
                    Cell::from(t.query_or_state()),
                ])
                .style(style)
            })
            .collect()
    };
    clamp_scroll(app, total, visible_rows);
    let header = Row::new(vec![
        right("Id"),
        Cell::from("User"),
        Cell::from("Host/IP"),
        Cell::from("DB"),
        right("Time"),
        Cell::from("Cmd"),
        Cell::from("Query or State"),
    ])
    .style(header_style());
    f.render_widget(Table::new(rows, widths).header(header).column_spacing(1), area);
}

fn draw_cmd(f: &mut Frame, app: &mut App, area: Rect) {
    let (t, area) = split_title(area);
    f.render_widget(Paragraph::new(title_line("Command Summary")), t);
    let visible_rows = area.height.saturating_sub(1) as usize;
    let offset = clamp_scroll(app, app.mon.cmd_rows.len(), visible_rows);
    let rows: Vec<Row> = app
        .mon
        .cmd_rows
        .iter()
        .skip(offset)
        .take(visible_rows)
        .map(|r| {
            Row::new(vec![
                right(r.name.clone()),
                right(r.total.to_string()),
                right(format!("{}%", r.pct)),
                Cell::from("|"),
                right(r.delta.to_string()),
                right(format!("{}%", r.delta_pct)),
            ])
            .style(if r.delta > 0 { Style::new().yellow() } else { Style::new() })
        })
        .collect();
    let header = Row::new(vec![
        right("Command"),
        right("Total"),
        right("Pct"),
        Cell::from("|"),
        right("Last"),
        right("Pct"),
    ])
    .style(header_style());
    let widths = [
        Constraint::Length(24),
        Constraint::Length(14),
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(10),
        Constraint::Length(5),
    ];
    f.render_widget(Table::new(rows, widths).header(header).column_spacing(1), area);
}

fn draw_status(f: &mut Frame, app: &mut App, area: Rect) {
    let (t, area) = split_title(area);
    let title = if app.mon.cfg.idle {
        "Status Counters"
    } else {
        "Status Counters (changed only - press i to show all)"
    };
    f.render_widget(Paragraph::new(title_line(title)), t);
    let visible_rows = area.height.saturating_sub(1) as usize;
    let offset = clamp_scroll(app, app.mon.status_rows.len(), visible_rows);
    let rows: Vec<Row> = app
        .mon
        .status_rows
        .iter()
        .skip(offset)
        .take(visible_rows)
        .map(|r| {
            let style = match r.change {
                Change::Up => Style::new().yellow(),
                Change::Down => Style::new().red(),
                Change::Same => Style::new(),
            };
            Row::new(vec![right(r.name.clone()), right(r.value.clone()), right(r.delta.clone())])
                .style(style)
        })
        .collect();
    let header = Row::new(vec![right("Counter"), right("Total"), right("Change")]).style(header_style());
    let widths = [Constraint::Length(40), Constraint::Length(20), Constraint::Length(14)];
    f.render_widget(Table::new(rows, widths).header(header).column_spacing(2), area);
}

fn draw_innodb(f: &mut Frame, app: &mut App, area: Rect) {
    let (t, area) = split_title(area);
    f.render_widget(Paragraph::new(title_line("InnoDB Status")), t);
    let total = app.mon.innodb.lines().count();
    let offset = clamp_scroll(app, total, area.height as usize);
    let text: Vec<Line> = app
        .mon
        .innodb
        .lines()
        .skip(offset)
        .take(area.height as usize)
        .map(|l| Line::from(l.to_string()))
        .collect();
    f.render_widget(Paragraph::new(text), area);
}

fn draw_qps(f: &mut Frame, app: &mut App, area: Rect) {
    let (t, area) = split_title(area);
    f.render_widget(Paragraph::new(title_line("Queries Per Second")), t);
    let hist = &app.mon.qps.history;
    let spark_h = (area.height / 3).clamp(3, 12);
    let [spark, stats, list] = Layout::vertical([
        Constraint::Length(spark_h),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(area);

    let width = spark.width as usize;
    let data: Vec<u64> = hist.iter().skip(hist.len().saturating_sub(width)).copied().collect();
    f.render_widget(
        Sparkline::default()
            .block(Block::new().borders(Borders::BOTTOM))
            .data(&data)
            .style(Style::new().yellow()),
        spark,
    );

    let (cur, max, avg) = if hist.is_empty() {
        (0, 0, 0.0)
    } else {
        let max = hist.iter().max().copied().unwrap_or(0);
        let avg = hist.iter().sum::<u64>() as f64 / hist.len() as f64;
        (*hist.back().unwrap(), max, avg)
    };
    f.render_widget(
        Paragraph::new(format!(
            " now: {cur}   max: {max}   avg: {avg:.1}   samples: {}",
            hist.len()
        ))
        .bold(),
        stats,
    );

    // mytop writes one number per line; show the newest at the bottom
    let n = list.height as usize;
    let lines: Vec<Line> = hist
        .iter()
        .skip(hist.len().saturating_sub(n))
        .map(|q| Line::from(q.to_string()))
        .collect();
    f.render_widget(Paragraph::new(lines), list);
    app.page = 1;
}

fn draw_bar(f: &mut Frame, app: &mut App, area: Rect) {
    if let Overlay::Prompt { kind, input } = &app.overlay {
        let line = Line::from(vec![
            Span::styled(kind.label(), Style::new().red().bold()),
            Span::raw(input.clone()),
        ]);
        f.render_widget(Paragraph::new(line), area);
        let x = area.x + (kind.label().chars().count() + input.chars().count()) as u16;
        f.set_cursor_position((x.min(area.right().saturating_sub(1)), area.y));
        return;
    }
    if matches!(app.overlay, Overlay::Pause) {
        f.render_widget(
            Paragraph::new("-- paused. press any key to resume --").style(Style::new().red().bold()),
            area,
        );
        return;
    }

    let right_text = " ?: help  q: quit ";
    let [left_area, right_area] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(right_text.len() as u16),
    ])
    .areas(area);

    let left: Line = if let Some(m) = app.current_message() {
        let style = if m.error {
            Style::new().white().on_red().bold()
        } else {
            Style::new().red().bold()
        };
        Line::from(Span::styled(format!(" {} ", m.text), style))
    } else {
        let cfg = &app.mon.cfg;
        let mut parts = vec![
            format!(" {}", app.mode),
            format!("delay {}s", if app.mode == Mode::Qps { 1 } else { cfg.delay }),
        ];
        if app.mode == Mode::Top {
            parts.push(format!("idle {}", if cfg.idle { "on" } else { "off" }));
            parts.push(format!("sort {}", if cfg.sort { "desc" } else { "asc" }));
            let fl = &app.mon.filters;
            if fl.any_active() {
                parts.push(format!("filter user={} db={} host={}", fl.user, fl.db, fl.host));
            }
            let shown = app.mon.visible_threads().len();
            parts.push(format!("threads {}/{}", shown, app.mon.threads.len()));
        }
        if app.mon.debug {
            parts.push(format!("refresh {} ms", app.last_refresh.as_millis()));
            if let Some(e) = &app.last_error {
                parts.push(format!("last error: {e}"));
            }
        }
        Line::from(parts.join(" · "))
    };
    let bar_style = Style::new().bg(Color::DarkGray).fg(Color::White);
    f.render_widget(Paragraph::new(left).style(bar_style), left_area);
    f.render_widget(Paragraph::new(right_text).style(bar_style.bold()), right_area);
}

fn centered(area: Rect, w_pct: u16, h_pct: u16) -> Rect {
    let w = (area.width * w_pct / 100).max(20.min(area.width));
    let h = (area.height * h_pct / 100).max(5.min(area.height));
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn draw_help(f: &mut Frame, area: Rect) {
    let mut lines = vec![
        Line::from(vec![
            Span::raw(format!("Help for rutop version {} - a Rust rewrite of mytop by Jeremy D. Zawodny", env!("CARGO_PKG_VERSION"))),
        ]),
        Line::from(""),
    ];
    for (k, d) in HELP {
        lines.push(Line::from(vec![
            Span::styled(format!("  {k:>12} "), Style::new().yellow().bold()),
            Span::raw(format!("- {d}")),
        ]));
    }
    let h = (lines.len() as u16 + 2).min(area.height);
    let w = 80.min(area.width);
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title(" Help ")
                .title_bottom(" press any key to resume "),
        ),
        rect,
    );
}

fn draw_text(f: &mut Frame, view: &mut TextView, area: Rect) {
    let rect = centered(area, 92, 90);
    let inner_w = rect.width.saturating_sub(2).max(1) as usize;
    let inner_h = rect.height.saturating_sub(2) as usize;
    let total: usize = if view.wrap {
        view.lines
            .iter()
            .map(|l| l.chars().count().div_ceil(inner_w).max(1))
            .sum()
    } else {
        view.lines.len()
    };
    view.scroll = view.scroll.min(total.saturating_sub(inner_h));
    let text = Text::from(view.lines.iter().map(|l| Line::from(l.clone())).collect::<Vec<_>>());
    let mut p = Paragraph::new(text)
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title(Span::styled(view.title.clone(), Style::new().bold()))
                .title_bottom(view.footer.clone()),
        )
        .scroll((view.scroll.min(u16::MAX as usize) as u16, 0));
    if view.wrap {
        p = p.wrap(Wrap { trim: false }).style(Style::new().yellow());
    }
    f.render_widget(Clear, rect);
    f.render_widget(p, rect);
}
