//! Plain-text rendering (printf-like, as in mytop) used by batch mode.

use crate::modes::{Change, CmdRow, StatusRow};
use crate::threads::Thread;

pub struct Ansi {
    pub on: bool,
}

impl Ansi {
    fn code(&self, c: &'static str) -> &'static str {
        if self.on {
            c
        } else {
            ""
        }
    }
    pub fn reset(&self) -> &'static str {
        self.code("\x1b[0m")
    }
    pub fn bold(&self) -> &'static str {
        self.code("\x1b[1m")
    }
    pub fn yellow(&self) -> &'static str {
        self.code("\x1b[33m")
    }
    pub fn red(&self) -> &'static str {
        self.code("\x1b[31m")
    }
    pub fn green(&self) -> &'static str {
        self.code("\x1b[32m")
    }
    pub fn white(&self) -> &'static str {
        self.code("\x1b[37m")
    }
}

/// Column widths of the thread list: Id, User, Host/IP, DB, Time, Cmd.
pub const THREAD_COLS: [usize; 6] = [9, 9, 15, 10, 9, 6];

pub fn threads(threads: &[&Thread], width: usize, c: &Ansi) -> Vec<String> {
    let used: usize = THREAD_COLS.iter().sum::<usize>() + THREAD_COLS.len();
    let free = width.saturating_sub(used).max(10);
    let mut out = Vec::with_capacity(threads.len() + 2);
    out.push(format!(
        "{}{:>9} {:>9} {:>15} {:>10} {:>9} {:>6} {:<free$}{}",
        c.bold(),
        "Id",
        "User",
        "Host/IP",
        "DB",
        "Time",
        "Cmd",
        "Query or State",
        c.reset()
    ));
    out.push(format!(
        "{:>9} {:>9} {:>15} {:>10} {:>9} {:>6} {:<free$}",
        "--", "----", "-------", "--", "----", "---", "----------"
    ));
    for t in threads {
        let color = match t.command.as_str() {
            "Query" => c.yellow(),
            "Sleep" => c.white(),
            "Connect" => c.green(),
            _ => "",
        };
        let info: String = t.query_or_state().chars().take(free).collect();
        out.push(format!(
            "{color}{:>9} {:>9.9} {:>15.15} {:>10.10} {:>9} {:>6.6} {:<free$}{}",
            t.id,
            t.user,
            t.host,
            t.db,
            t.time,
            t.command,
            info,
            if color.is_empty() { "" } else { c.reset() }
        ));
    }
    out
}

pub fn cmd_summary(rows: &[CmdRow]) -> Vec<String> {
    let mut out = vec![
        format!("{:>18} {:>10} {:>4}  | {:>5} {:>4}", "Command", "Total", "Pct", "Last", "Pct"),
        format!("{:>18} {:>10} {:>4}  | {:>5} {:>4}", "-------", "-----", "---", "----", "---"),
    ];
    for r in rows {
        out.push(format!(
            "{:>18} {:>10} {:>4}  | {:>5} {:>4}",
            r.name,
            r.total,
            format!("{}%", r.pct),
            r.delta,
            format!("{}%", r.delta_pct)
        ));
    }
    out
}

pub fn status(rows: &[StatusRow], c: &Ansi) -> Vec<String> {
    let mut out = vec![
        format!("{:>32}  {:>10} {:>10}", "Counter", "Total", "Change"),
        format!("{:>32}  {:>10} {:>10}", "-------", "-----", "------"),
    ];
    for r in rows {
        let color = match r.change {
            Change::Up => c.yellow(),
            Change::Down => c.red(),
            Change::Same => "",
        };
        out.push(format!(
            "{color}{:>32}: {:>10} {:>10}{}",
            r.name,
            r.value,
            r.delta,
            if color.is_empty() { "" } else { c.reset() }
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_line_layout() {
        let t = Thread {
            id: 61,
            user: "jzawodn".into(),
            host: "localhost".into(),
            db: "music".into(),
            time: 0,
            command: "Query".into(),
            info: "show processlist".into(),
            ..Default::default()
        };
        let lines = threads(&[&t], 80, &Ansi { on: false });
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[2].trim_end(),
            "       61   jzawodn       localhost      music         0  Query show processlist"
        );
        assert_eq!(lines[2].len(), 80);
    }
}
