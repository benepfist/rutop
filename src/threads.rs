//! Processing of `SHOW FULL PROCESSLIST` rows.

use std::net::IpAddr;

use crate::db::Row;
use crate::filter::Filters;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Thread {
    pub id: u64,
    pub user: String,
    /// Display host: IP without port, or the first label of the hostname.
    pub host: String,
    /// Set if the client host is an IP address (candidate for resolving).
    pub ip: Option<IpAddr>,
    pub db: String,
    pub time: i64,
    pub command: String,
    pub state: String,
    /// The full query as reported by the server (leading whitespace removed).
    pub info: String,
}

impl Thread {
    pub fn from_row(row: &Row) -> Thread {
        let (host, ip) = shorten_host(row.str("Host"));
        Thread {
            id: row.str("Id").trim().parse().unwrap_or(0),
            user: row.str("User").to_string(),
            host,
            ip,
            db: row.str("db").to_string(),
            time: row
                .str("Time")
                .trim()
                .parse::<f64>()
                .map(|t| t as i64)
                .unwrap_or(0),
            command: row.str("Command").to_string(),
            state: row.str("State").to_string(),
            info: row.str("Info").trim_start().to_string(),
        }
    }

    /// Query (normalized to one line) or, if none, the thread state.
    pub fn query_or_state(&self) -> String {
        if !self.info.is_empty() {
            normalize_query(&self.info)
        } else {
            normalize_query(&self.state)
        }
    }

    pub fn is_idle(&self) -> bool {
        self.command == "Sleep" || self.command == "Binlog Dump"
    }
}

/// Remove newlines, collapse whitespace and replace binary junk so the
/// terminal doesn't freak out.
pub fn normalize_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for c in s.trim_start().chars() {
        if c == '\n' || c == '\r' {
            continue;
        }
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
            continue;
        }
        last_space = false;
        out.push(if c.is_control() || c == '\u{FFFD}' { '?' } else { c });
    }
    out
}

/// Drop the domain name unless the host looks like an IP address; strip the
/// port number because it's rarely interesting.
pub fn shorten_host(raw: &str) -> (String, Option<IpAddr>) {
    let raw = raw.trim();
    if raw.is_empty() {
        return (String::new(), None);
    }
    if let Ok(ip) = raw.parse::<IpAddr>() {
        return (ip.to_string(), Some(ip));
    }
    // "host:port", "1.2.3.4:port", "[::1]:port" or "::1:port"
    if let Some((h, port)) = raw.rsplit_once(':') {
        if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) {
            let h = h.trim_start_matches('[').trim_end_matches(']');
            if let Ok(ip) = h.parse::<IpAddr>() {
                return (ip.to_string(), Some(ip));
            }
            if !h.contains(':') {
                return (first_label(h), None);
            }
        }
    }
    (first_label(raw), None)
}

pub fn first_label(host: &str) -> String {
    host.split('.').next().unwrap_or(host).to_string()
}

/// Threads to display: filtered and sorted by time.
pub fn visible<'a>(
    threads: &'a [Thread],
    idle: bool,
    reverse: bool,
    filters: &Filters,
) -> Vec<&'a Thread> {
    let mut v: Vec<&Thread> = threads
        .iter()
        .filter(|t| idle || !t.is_idle())
        .filter(|t| filters.user.matches(&t.user))
        .filter(|t| filters.db.matches(&t.db))
        .filter(|t| filters.host.matches(&t.host))
        .collect();
    if reverse {
        v.sort_by_key(|t| std::cmp::Reverse(t.time));
    } else {
        v.sort_by_key(|t| t.time);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::Filter;

    #[test]
    fn hosts() {
        assert_eq!(shorten_host("10.0.0.5:51234"), ("10.0.0.5".into(), Some("10.0.0.5".parse().unwrap())));
        assert_eq!(shorten_host("web1.example.com:3306").0, "web1");
        assert_eq!(shorten_host("localhost").0, "localhost");
        assert_eq!(shorten_host("localhost:4711").0, "localhost");
        assert_eq!(shorten_host("[fe80::1]:4711").0, "fe80::1");
        assert_eq!(shorten_host("").0, "");
    }

    #[test]
    fn normalize() {
        assert_eq!(
            normalize_query("  SELECT *\n  FROM\tt\r\n WHERE a=1\u{1}"),
            "SELECT * FROM t WHERE a=1?"
        );
    }

    fn t(id: u64, user: &str, cmd: &str, time: i64) -> Thread {
        Thread {
            id,
            user: user.into(),
            command: cmd.into(),
            time,
            ..Default::default()
        }
    }

    #[test]
    fn filtering_and_sorting() {
        let threads = vec![
            t(1, "app", "Query", 5),
            t(2, "app", "Sleep", 100),
            t(3, "root", "Query", 1),
            t(4, "repl", "Binlog Dump", 900),
        ];
        let all = Filters {
            user: Filter::Any,
            db: Filter::Any,
            host: Filter::Any,
        };
        let ids = |v: Vec<&Thread>| v.iter().map(|t| t.id).collect::<Vec<_>>();
        assert_eq!(ids(visible(&threads, true, false, &all)), vec![3, 1, 2, 4]);
        assert_eq!(ids(visible(&threads, false, true, &all)), vec![1, 3]);
        let only_app = Filters {
            user: Filter::from_input("app").unwrap(),
            ..all.clone()
        };
        assert_eq!(ids(visible(&threads, true, true, &only_app)), vec![2, 1]);
    }

    #[test]
    fn from_row() {
        let row = Row::from_pairs(&[
            ("Id", Some("42")),
            ("User", Some("bob")),
            ("Host", Some("db.example.org:5555")),
            ("db", None),
            ("Command", Some("Query")),
            ("Time", Some("7")),
            ("State", Some("executing")),
            ("Info", Some("  select 1")),
        ]);
        let t = Thread::from_row(&row);
        assert_eq!(t.id, 42);
        assert_eq!(t.host, "db");
        assert_eq!(t.db, "");
        assert_eq!(t.info, "select 1");
        assert_eq!(t.query_or_state(), "select 1");
    }
}
