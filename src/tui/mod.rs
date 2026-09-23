//! Interactive terminal UI.

mod app;
mod input;
mod render;

use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::monitor::{Monitor, RefreshError};
use app::{App, Overlay};

pub fn refresh_error_text(e: RefreshError) -> String {
    match e {
        RefreshError::Fatal(m) | RefreshError::Query(m) => m,
    }
}

pub fn run(mon: Monitor) -> Result<()> {
    let mut app = App::new(mon);
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    let mut next_refresh = Instant::now();
    loop {
        if app.quit {
            return Ok(());
        }

        // Like mytop, the display is frozen while a prompt or popup is open.
        let may_refresh = matches!(app.overlay, Overlay::None);
        if may_refresh && (app.force_refresh || Instant::now() >= next_refresh) {
            app.force_refresh = false;
            let started = Instant::now();
            match app.mon.refresh(app.mode) {
                Ok(()) => {}
                Err(RefreshError::Fatal(e)) => {
                    return Err(anyhow!("lost connection to MySQL server: {e}"));
                }
                Err(RefreshError::Query(e)) => {
                    app.last_error = Some(e.clone());
                    app.error(e);
                }
            }
            app.last_refresh = started.elapsed();
            next_refresh = Instant::now() + app.interval();
        }

        terminal.draw(|f| render::draw(f, app))?;

        let timeout = next_refresh
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(250));
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    input::handle_key(app, key);
                }
                _ => {}
            }
        }
    }
}
