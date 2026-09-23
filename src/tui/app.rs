use std::time::{Duration, Instant};

use crate::config::Mode;
use crate::monitor::Monitor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prompt {
    Delay,
    FilterUser,
    FilterDb,
    FilterHost,
    Kill,
    KillUser,
    FullQuery,
    Explain,
}

impl Prompt {
    pub fn label(self) -> &'static str {
        match self {
            Prompt::Delay => "Seconds of Delay: ",
            Prompt::FilterUser => "Which user (blank for all, /.../ for regex): ",
            Prompt::FilterDb => "Which database (blank for all, /.../ for regex): ",
            Prompt::FilterHost => "Which hostname (blank for all, /.../ for regex): ",
            Prompt::Kill => "Thread id to kill: ",
            Prompt::KillUser => "User to kill: ",
            Prompt::FullQuery => "Full query for which thread id: ",
            Prompt::Explain => "Explain which query (id): ",
        }
    }
}

pub struct TextView {
    pub title: String,
    pub lines: Vec<String>,
    pub footer: String,
    pub wrap: bool,
    pub scroll: usize,
    /// For the full query view: `e` explains this thread.
    pub explain_id: Option<u64>,
}

pub enum Overlay {
    None,
    Help,
    Pause,
    Prompt { kind: Prompt, input: String },
    Text(TextView),
}

pub struct Message {
    pub text: String,
    pub error: bool,
    pub at: Instant,
}

pub struct App {
    pub mon: Monitor,
    pub mode: Mode,
    pub overlay: Overlay,
    pub message: Option<Message>,
    /// Scroll offset of the main view.
    pub scroll: usize,
    /// Rows visible in the main view (set by the renderer, used for paging).
    pub page: usize,
    pub quit: bool,
    pub force_refresh: bool,
    pub last_error: Option<String>,
    pub last_refresh: Duration,
}

impl App {
    pub fn new(mon: Monitor) -> App {
        let mode = mon.cfg.mode;
        App {
            mon,
            mode,
            overlay: Overlay::None,
            message: None,
            scroll: 0,
            page: 10,
            quit: false,
            force_refresh: true,
            last_error: None,
            last_refresh: Duration::ZERO,
        }
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: false,
            at: Instant::now(),
        });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
            at: Instant::now(),
        });
    }

    pub fn current_message(&self) -> Option<&Message> {
        self.message
            .as_ref()
            .filter(|m| m.at.elapsed() < Duration::from_secs(if m.error { 5 } else { 2 }))
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.scroll = 0;
        }
        self.force_refresh = true;
    }

    pub fn show_text(&mut self, title: impl Into<String>, lines: Vec<String>) {
        self.overlay = Overlay::Text(TextView {
            title: title.into(),
            lines,
            footer: " ↑↓ PgUp PgDn: scroll · any other key: resume ".into(),
            wrap: false,
            scroll: 0,
            explain_id: None,
        });
    }

    /// Refresh interval of the current mode.
    pub fn interval(&self) -> Duration {
        match self.mode {
            Mode::Qps => Duration::from_secs(1),
            _ => Duration::from_secs(self.mon.cfg.delay.max(1)),
        }
    }
}
