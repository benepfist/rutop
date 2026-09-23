//! State and computations of the secondary modes: command summary (`c`),
//! status counters (`S`) and queries per second (`m`).

use std::collections::{HashMap, VecDeque};

use crate::db::Row;

#[derive(Clone, Debug, PartialEq)]
pub struct CmdRow {
    pub name: String,
    pub total: u64,
    pub pct: u64,
    pub delta: u64,
    pub delta_pct: u64,
}

/// Summary of the `Com_*` counters with deltas since the previous refresh.
#[derive(Default)]
pub struct CmdSummary {
    prev: Option<HashMap<String, u64>>,
}

impl CmdSummary {
    pub fn reset(&mut self) {
        self.prev = None;
    }

    pub fn update(&mut self, rows: &[Row]) -> Vec<CmdRow> {
        let mut data: Vec<(String, u64)> = rows
            .iter()
            .filter_map(|r| {
                let name = r.str("Variable_name");
                let value: u64 = r.str("Value").trim().parse().ok()?;
                if value == 0 {
                    return None;
                }
                let name = name.strip_prefix("Com_").unwrap_or(name).replace('_', " ");
                Some((name, value))
            })
            .collect();

        let total: u64 = data.iter().map(|(_, v)| v).sum();
        let deltas: Vec<u64> = data
            .iter()
            .map(|(name, v)| match &self.prev {
                Some(prev) => v.saturating_sub(prev.get(name).copied().unwrap_or(0)),
                None => 0,
            })
            .collect();
        let delta_total: u64 = deltas.iter().sum();

        let pct = |v: u64, t: u64| (v * 100).checked_div(t).unwrap_or(0);
        self.prev = Some(data.iter().cloned().collect());

        let mut out: Vec<CmdRow> = data
            .drain(..)
            .zip(deltas)
            .map(|((name, total_v), delta)| CmdRow {
                name,
                total: total_v,
                pct: pct(total_v, total),
                delta,
                delta_pct: pct(delta, delta_total),
            })
            .collect();
        out.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.name.cmp(&b.name)));
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Up,
    Down,
    Same,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatusRow {
    pub name: String,
    pub value: String,
    pub delta: String,
    pub change: Change,
}

/// `SHOW GLOBAL STATUS` counters with the change since the previous refresh.
#[derive(Default)]
pub struct StatusView {
    cache: HashMap<String, String>,
}

impl StatusView {
    pub fn reset(&mut self) {
        self.cache.clear();
    }

    /// `hide_unchanged` corresponds to mytop's behaviour with idle display off.
    pub fn update(&mut self, rows: &[Row], hide_unchanged: bool) -> Vec<StatusRow> {
        let mut out = Vec::new();
        for r in rows {
            let name = r.str("Variable_name");
            let value = r.str("Value").trim();
            if name.starts_with("Com_") {
                continue; // skip Com_ stats (see command summary)
            }
            if !value.bytes().any(|b| b.is_ascii_digit()) {
                continue; // skip non-numeric
            }
            let old = self.cache.insert(name.to_string(), value.to_string());
            let (delta, change) = match (old.as_deref().and_then(parse_num), parse_num(value)) {
                (Some(o), Some(v)) => {
                    let d = v - o;
                    let change = if d > 0.0 {
                        Change::Up
                    } else if d < 0.0 {
                        Change::Down
                    } else {
                        Change::Same
                    };
                    (fmt_num(d), change)
                }
                _ => ("0".to_string(), Change::Same),
            };
            if hide_unchanged && old.is_some() && change == Change::Same {
                continue;
            }
            out.push(StatusRow {
                name: name.to_string(),
                value: value.to_string(),
                delta,
                change,
            });
        }
        out
    }
}

fn parse_num(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n:.3}")
    }
}

/// Queries per second, sampled once per second.
#[derive(Default)]
pub struct Qps {
    last: Option<u64>,
    pub history: VecDeque<u64>,
}

impl Qps {
    const MAX: usize = 3600;

    pub fn reset(&mut self) {
        self.last = None;
    }

    /// Feed the current `Questions` counter; returns the new qps value
    /// (nothing on the first sample).
    pub fn update(&mut self, questions: u64) -> Option<u64> {
        let prev = self.last.replace(questions)?;
        let qps = questions.saturating_sub(prev);
        if self.history.len() == Self::MAX {
            self.history.pop_front();
        }
        self.history.push_back(qps);
        Some(qps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(pairs: &[(&str, &str)]) -> Vec<Row> {
        pairs
            .iter()
            .map(|(k, v)| Row::from_pairs(&[("Variable_name", Some(k)), ("Value", Some(v))]))
            .collect()
    }

    #[test]
    fn cmd_summary() {
        let mut c = CmdSummary::default();
        let first = c.update(&rows(&[
            ("Com_select", "300"),
            ("Com_insert", "100"),
            ("Com_alter_table", "0"),
        ]));
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].name, "select");
        assert_eq!(first[0].pct, 75);
        assert_eq!(first[0].delta, 0);

        let second = c.update(&rows(&[
            ("Com_select", "330"),
            ("Com_insert", "110"),
            ("Com_show_status", "10"),
        ]));
        let get = |n: &str| second.iter().find(|r| r.name == n).unwrap().clone();
        assert_eq!(get("select").delta, 30);
        assert_eq!(get("select").delta_pct, 60);
        assert_eq!(get("show status").delta, 10);
    }

    #[test]
    fn status_view() {
        let mut s = StatusView::default();
        let r = s.update(
            &rows(&[("Uptime", "10"), ("Com_select", "1"), ("Ssl_cipher", ""), ("Open_tables", "5")]),
            true,
        );
        assert_eq!(r.len(), 2);
        let r = s.update(&rows(&[("Uptime", "12"), ("Open_tables", "5")]), true);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].delta, "2");
        assert_eq!(r[0].change, Change::Up);
        let r = s.update(&rows(&[("Uptime", "12"), ("Open_tables", "4")]), false);
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].change, Change::Down);
    }

    #[test]
    fn qps() {
        let mut q = Qps::default();
        assert_eq!(q.update(100), None);
        assert_eq!(q.update(150), Some(50));
        assert_eq!(q.history.len(), 1);
    }
}
