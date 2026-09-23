use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, Overlay, Prompt, TextView};
use crate::config::Mode;
use crate::filter::Filter;

pub fn handle_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
    {
        app.quit = true;
        return;
    }

    match std::mem::replace(&mut app.overlay, Overlay::None) {
        Overlay::None => normal_key(app, key),
        Overlay::Help | Overlay::Pause => app.force_refresh = true,
        Overlay::Prompt { kind, mut input } => match key.code {
            KeyCode::Enter => {
                submit(app, kind, &input);
                app.force_refresh = true;
            }
            KeyCode::Esc => app.force_refresh = true,
            KeyCode::Backspace => {
                input.pop();
                app.overlay = Overlay::Prompt { kind, input };
            }
            KeyCode::Char(c) => {
                input.push(c);
                app.overlay = Overlay::Prompt { kind, input };
            }
            _ => app.overlay = Overlay::Prompt { kind, input },
        },
        Overlay::Text(mut view) => {
            if scroll(&mut view.scroll, key.code, 20) {
                app.overlay = Overlay::Text(view);
            } else if let (KeyCode::Char('e'), Some(id)) = (key.code, view.explain_id) {
                explain(app, id);
            } else {
                app.force_refresh = true;
            }
        }
    }
}

/// Apply a scroll key; returns false if the key is not a scroll key.
fn scroll(pos: &mut usize, code: KeyCode, page: usize) -> bool {
    match code {
        KeyCode::Up => *pos = pos.saturating_sub(1),
        KeyCode::Down => *pos = pos.saturating_add(1),
        KeyCode::PageUp => *pos = pos.saturating_sub(page),
        KeyCode::PageDown => *pos = pos.saturating_add(page),
        KeyCode::Home => *pos = 0,
        KeyCode::End => *pos = usize::MAX,
        _ => return false,
    }
    true
}

fn prompt(app: &mut App, kind: Prompt) {
    app.overlay = Overlay::Prompt {
        kind,
        input: String::new(),
    };
}

fn normal_key(app: &mut App, key: KeyEvent) {
    let page = app.page.max(1);
    if scroll(&mut app.scroll, key.code, page) {
        return;
    }
    let KeyCode::Char(c) = key.code else {
        if key.code == KeyCode::Esc && app.mode != Mode::Top {
            app.set_mode(Mode::Top);
        }
        return;
    };
    let cfg = &mut app.mon.cfg;
    match c {
        't' | 'T' => app.set_mode(Mode::Top),
        'q' => {
            if app.mode == Mode::Top {
                app.quit = true;
            } else {
                app.set_mode(Mode::Top);
            }
        }
        'm' => app.set_mode(Mode::Qps),
        'c' => app.set_mode(Mode::Cmd),
        'I' => app.set_mode(Mode::Innodb),
        'S' => app.set_mode(Mode::Status),
        's' => prompt(app, Prompt::Delay),
        'u' => prompt(app, Prompt::FilterUser),
        'd' => prompt(app, Prompt::FilterDb),
        'h' => prompt(app, Prompt::FilterHost),
        'k' => prompt(app, Prompt::Kill),
        'K' => prompt(app, Prompt::KillUser),
        'f' => prompt(app, Prompt::FullQuery),
        'e' => prompt(app, Prompt::Explain),
        'R' => {
            cfg.resolve = !cfg.resolve;
            let msg = if cfg.resolve {
                "-- resolving IP addresses --"
            } else {
                "-- not resolving IP addresses --"
            };
            app.info(msg);
            app.force_refresh = true;
        }
        'F' => {
            app.mon.filters.clear();
            app.info("-- display unfiltered --");
            app.force_refresh = true;
        }
        'p' => app.overlay = Overlay::Pause,
        'i' => {
            cfg.idle = !cfg.idle;
            cfg.sort = !cfg.idle;
            let msg = if cfg.idle {
                "-- idle (sleeping) processes unfiltered --"
            } else {
                "-- idle (sleeping) processes filtered --"
            };
            app.info(msg);
            app.force_refresh = true;
        }
        'o' => {
            cfg.sort = !cfg.sort;
            app.info("-- sort order reversed --");
            app.force_refresh = true;
        }
        'H' => {
            cfg.header = !cfg.header;
            app.force_refresh = true;
        }
        'L' => {
            cfg.long_nums = !cfg.long_nums;
            let msg = if cfg.long_nums {
                "-- long numbers --"
            } else {
                "-- short numbers --"
            };
            app.info(msg);
        }
        '#' => {
            app.mon.debug = !app.mon.debug;
            let msg = if app.mon.debug { "-- debug on --" } else { "-- debug off --" };
            app.info(msg);
        }
        '?' => app.overlay = Overlay::Help,
        'r' => match app.mon.flush_status() {
            Ok(()) => {
                app.info("-- counters reset --");
                app.force_refresh = true;
            }
            Err(e) => app.error(format!("FLUSH STATUS failed: {e}")),
        },
        'D' => {
            let lines = app.mon.cfg.dump();
            app.show_text(" Configuration ", lines);
        }
        'V' => match app.mon.variables() {
            Ok(lines) => app.show_text(" SHOW VARIABLES ", lines),
            Err(e) => app.error(super::refresh_error_text(e)),
        },
        _ => {}
    }
}

fn parse_id(input: &str) -> Option<u64> {
    let id: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
        id.parse().ok()
    } else {
        None
    }
}

fn submit(app: &mut App, kind: Prompt, input: &str) {
    match kind {
        Prompt::Delay => {
            let digits: String = input
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(secs) = digits.parse::<u64>() {
                app.mon.cfg.delay = secs.max(1);
            }
        }
        Prompt::FilterUser | Prompt::FilterDb | Prompt::FilterHost => {
            match Filter::from_input(input) {
                Ok(f) => {
                    let filters = &mut app.mon.filters;
                    match kind {
                        Prompt::FilterUser => filters.user = f,
                        Prompt::FilterDb => filters.db = f,
                        _ => filters.host = f,
                    }
                }
                Err(e) => app.error(format!("-- {e:#} --")),
            }
        }
        Prompt::Kill => match parse_id(input) {
            Some(id) => match app.mon.kill(id) {
                Ok(()) => app.info(format!("-- killed thread {id} --")),
                Err(e) => app.error(format!("KILL {id} failed: {e}")),
            },
            None => app.error("-- invalid thread id --"),
        },
        Prompt::KillUser => {
            let user: String = input.chars().filter(|c| !c.is_whitespace()).collect();
            if user.is_empty() {
                app.error("-- invalid user --");
                return;
            }
            let (killed, errors) = app.mon.kill_user(&user);
            if errors.is_empty() {
                app.info(format!("-- killed {killed} thread(s) of user {user} --"));
            } else {
                app.error(format!(
                    "-- killed {killed} thread(s) of user {user}, {} failed: {} --",
                    errors.len(),
                    errors.join("; ")
                ));
            }
        }
        Prompt::FullQuery => {
            let Some(t) = parse_id(input).and_then(|id| app.mon.thread(id)) else {
                app.error("*** Invalid id. ***");
                return;
            };
            let lines: Vec<String> = if t.info.is_empty() {
                vec!["(no query)".into()]
            } else {
                t.info.lines().map(str::to_string).collect()
            };
            app.overlay = Overlay::Text(TextView {
                title: format!(" Thread {} was executing following query: ", t.id),
                lines,
                footer: " e: explain · ↑↓: scroll · any other key: resume ".into(),
                wrap: true,
                scroll: 0,
                explain_id: Some(t.id),
            });
        }
        Prompt::Explain => match parse_id(input) {
            Some(id) => explain(app, id),
            None => app.error("*** Invalid id. ***"),
        },
    }
}

fn explain(app: &mut App, id: u64) {
    match app.mon.explain(id) {
        Ok(lines) => app.show_text(format!(" EXPLAIN (thread {id}) "), lines),
        Err(e) => app.error(e),
    }
}
