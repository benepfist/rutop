//! Batch mode: no screen handling, output once to stdout (qps mode loops).

use std::io::{self, IsTerminal, Write};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};

use crate::config::Mode;
use crate::monitor::{Monitor, RefreshError};
use crate::text::{self, Ansi};

fn err(e: RefreshError) -> anyhow::Error {
    match e {
        RefreshError::Fatal(m) | RefreshError::Query(m) => anyhow!(m),
    }
}

pub fn run(mut mon: Monitor) -> Result<()> {
    let stdout = io::stdout();
    let tty = stdout.is_terminal();
    let color = mon.cfg.color && tty && ansi_ok();
    let c = Ansi { on: color };
    let width = if tty {
        ratatui::crossterm::terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(80)
    } else {
        80
    };
    let mut out = stdout.lock();

    let lines = match mon.cfg.mode {
        Mode::Qps => loop {
            if let Some(qps) = mon.refresh_qps().map_err(err)? {
                writeln!(out, "{qps}")?;
                out.flush()?;
            }
            thread::sleep(Duration::from_secs(1));
        },
        Mode::Top => {
            mon.refresh(Mode::Top).map_err(err)?;
            let mut lines = Vec::new();
            if mon.cfg.header {
                lines.extend(mon.header_lines(width));
                lines.push(String::new());
            }
            lines.extend(text::threads(&mon.visible_threads(), width, &c));
            lines
        }
        Mode::Cmd => {
            mon.refresh(Mode::Cmd).map_err(err)?;
            text::cmd_summary(&mon.cmd_rows)
        }
        Mode::Status => {
            mon.refresh(Mode::Status).map_err(err)?;
            text::status(&mon.status_rows, &c)
        }
        Mode::Innodb => {
            mon.refresh(Mode::Innodb).map_err(err)?;
            mon.innodb.lines().map(str::to_string).collect()
        }
    };

    for l in lines {
        writeln!(out, "{}", l.trim_end())?;
    }
    Ok(())
}

fn ansi_ok() -> bool {
    #[cfg(windows)]
    {
        ratatui::crossterm::ansi_support::supports_ansi()
    }
    #[cfg(not(windows))]
    {
        true
    }
}
