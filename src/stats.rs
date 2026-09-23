//! Header statistics computed from `SHOW GLOBAL STATUS` snapshots, plus
//! number formatting helpers (`make_short`, `commify`).

use std::collections::HashMap;

pub type StatusMap = HashMap<String, String>;

pub fn num(m: &StatusMap, key: &str) -> f64 {
    m.get(key)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn div(a: f64, b: f64) -> f64 {
    if b == 0.0 || !b.is_finite() {
        0.0
    } else {
        a / b
    }
}

/// Insert thousands separators ("1234567" -> "1,234,567").
pub fn commify(n: f64) -> String {
    let s = if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n:.2}")
    };
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", s.as_str()),
    };
    let (int, frac) = match rest.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (rest, None),
    };
    let mut out = String::with_capacity(int.len() + int.len() / 3);
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    match frac {
        Some(f) => format!("{sign}{out}.{f}"),
        None => format!("{sign}{out}"),
    }
}

/// Compact numeric representation (10,000 -> 9.8k), or commified if `long`.
pub fn make_short(number: f64, long: bool) -> String {
    if long {
        return commify(number);
    }
    const UNITS: [&str; 5] = ["", "k", "M", "G", "T"];
    let mut n = number;
    let mut i = 0;
    while n > 1025.0 && i < UNITS.len() - 1 {
        n /= 1024.0;
        i += 1;
    }
    format!("{:.1}{}", n, UNITS[i])
}

/// Server uptime as `d+hh:mm:ss`.
pub fn fmt_uptime(secs: f64) -> String {
    let t = secs.max(0.0) as u64;
    let d = t / 86_400;
    let h = (t % 86_400) / 3600;
    let m = (t % 3600) / 60;
    let s = t % 60;
    format!("{d}+{h:02}:{m:02}:{s:02}")
}

/// Select/Insert/Update/Delete percentages.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Siud {
    pub select: f64,
    pub insert: f64,
    pub update: f64,
    pub delete: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NowStats {
    pub qps: f64,
    pub slow_qps: f64,
    pub siud: Siud,
    pub bytes_in: f64,
    pub bytes_out: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct QcacheStats {
    pub hits: f64,
    pub hits_per_sec: f64,
    pub hits_now: f64,
    pub ratio: f64,
    pub ratio_now: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub uptime: f64,
    pub questions: f64,
    pub avg_qps: f64,
    pub slow: f64,
    pub siud: Siud,
    pub threads_connected: f64,
    pub threads_running: f64,
    pub threads_cached: f64,
    pub key_efficiency: f64,
    pub bytes_in_avg: f64,
    pub bytes_out_avg: f64,
    /// Only available from the second sample on.
    pub now: Option<NowStats>,
    /// Only if the query cache is enabled and has hits.
    pub qcache: Option<QcacheStats>,
}

/// Compute the header from the current and previous status snapshot.
/// `t_delta` is the time in seconds between both snapshots.
pub fn compute_header(
    cur: &StatusMap,
    old: Option<&StatusMap>,
    t_delta: Option<f64>,
    have_query_cache: bool,
) -> Header {
    let g = |k: &str| num(cur, k);
    let uptime = g("Uptime");
    let questions = g("Questions");
    let qc_hits = if have_query_cache { g("Qcache_hits") } else { 0.0 };

    let key_read_requests = if g("Key_read_requests") == 0.0 {
        1.0
    } else {
        g("Key_read_requests")
    };

    let siud = Siud {
        // a Qcache hit is really a select and should be counted
        select: div(100.0 * (g("Com_select") + qc_hits), questions),
        insert: div(100.0 * (g("Com_insert") + g("Com_replace")), questions),
        update: div(100.0 * g("Com_update"), questions),
        delete: div(100.0 * g("Com_delete"), questions),
    };

    let now = match (old, t_delta) {
        (Some(old), Some(dt)) if dt > 0.0 => {
            let o = |k: &str| num(old, k);
            let d = |k: &str| g(k) - o(k);
            let old_qc = if have_query_cache { o("Qcache_hits") } else { 0.0 };
            let q_diff = d("Questions");
            Some(NowStats {
                qps: q_diff / dt,
                slow_qps: d("Slow_queries") / dt,
                siud: Siud {
                    select: div(100.0 * (d("Com_select") + qc_hits - old_qc), q_diff),
                    insert: div(100.0 * (d("Com_insert") + d("Com_replace")), q_diff),
                    update: div(100.0 * d("Com_update"), q_diff),
                    delete: div(100.0 * d("Com_delete"), q_diff),
                },
                bytes_in: d("Bytes_received") / dt,
                bytes_out: d("Bytes_sent") / dt,
            })
        }
        _ => None,
    };

    let qcache = if have_query_cache && g("Com_select") > 0.0 && qc_hits > 0.0 {
        let (hits_now, ratio_now) = match (old, t_delta) {
            (Some(old), Some(dt)) if dt > 0.0 => {
                let old_hits = num(old, "Qcache_hits");
                let old_sel = num(old, "Com_select");
                let sel_delta = g("Com_select") + qc_hits - (old_hits + old_sel);
                (
                    (qc_hits - old_hits) / dt,
                    100.0 * (qc_hits - old_hits) / if sel_delta == 0.0 { 1.0 } else { sel_delta },
                )
            }
            _ => (0.0, 0.0),
        };
        Some(QcacheStats {
            hits: qc_hits,
            hits_per_sec: div(qc_hits, uptime),
            hits_now,
            ratio: div(100.0 * qc_hits, qc_hits + g("Com_select")),
            ratio_now,
        })
    } else {
        None
    };

    Header {
        uptime,
        questions,
        avg_qps: div(questions, uptime),
        slow: g("Slow_queries"),
        siud,
        threads_connected: g("Threads_connected"),
        threads_running: g("Threads_running"),
        threads_cached: g("Threads_cached"),
        key_efficiency: 100.0 - (g("Key_reads") / key_read_requests) * 100.0,
        bytes_in_avg: div(g("Bytes_received"), uptime),
        bytes_out_avg: div(g("Bytes_sent"), uptime),
        now,
        qcache,
    }
}

fn siud_str(s: &Siud) -> String {
    format!(
        "{:02.0}/{:02.0}/{:02.0}/{:02.0}",
        s.select, s.insert, s.update, s.delete
    )
}

/// Format the header lines (same layout as mytop).
pub fn format_header(
    h: &Header,
    host: &str,
    version: &str,
    clock: &str,
    width: usize,
    long: bool,
) -> Vec<String> {
    let short = |n: f64| make_short(n, long);
    let mut lines = Vec::with_capacity(5);

    let host_width = 52usize;
    let up_width = width.saturating_sub(host_width);
    lines.push(format!(
        "{:<host_width$}{:>up_width$}",
        format!("MySQL on {host} ({version})"),
        format!("up {} [{}]", fmt_uptime(h.uptime), clock),
    ));

    lines.push(format!(
        " Queries: {:<5}  qps: {:>4.0} Slow: {:>7}         Se/In/Up/De(%):    {} ",
        short(h.questions),
        h.avg_qps,
        short(h.slow),
        siud_str(&h.siud),
    ));

    let (qps_now, slow_now, siud_now) = match &h.now {
        Some(n) => (
            format!("{:>4.0}", n.qps),
            format!("{:>3.1}", n.slow_qps),
            siud_str(&n.siud),
        ),
        None => ("   -".into(), "  -".into(), "--/--/--/--".into()),
    };
    lines.push(format!(
        "             qps now: {} Slow qps: {}  Threads: {:>4.0} ({:>4.0}/{:>4.0}) {} ",
        qps_now, slow_now, h.threads_connected, h.threads_running, h.threads_cached, siud_now,
    ));

    if let Some(q) = &h.qcache {
        lines.push(format!(
            " Cache Hits: {:<5} Hits/s: {:>4.1} Hits now: {:>5.1}  Ratio: {:>4.1}% Ratio now: {:>4.1}% ",
            short(q.hits),
            q.hits_per_sec,
            q.hits_now,
            q.ratio,
            q.ratio_now,
        ));
    }

    let mut last = format!(
        " Key Efficiency: {:2.1}%  Bps in/out: {:>5}/{:>5}   ",
        h.key_efficiency,
        short(h.bytes_in_avg),
        short(h.bytes_out_avg),
    );
    if let Some(n) = &h.now {
        last.push_str(&format!(
            "Now in/out: {:>5}/{:>5}",
            short(n.bytes_in),
            short(n.bytes_out)
        ));
    }
    lines.push(last);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> StatusMap {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn short_numbers() {
        assert_eq!(make_short(5.0, false), "5.0");
        assert_eq!(make_short(1025.0, false), "1025.0");
        assert_eq!(make_short(10_000.0, false), "9.8k");
        assert_eq!(make_short(20_237_516.0, false), "19.3M");
        assert_eq!(make_short(20_237_516.0, true), "20,237,516");
    }

    #[test]
    fn commas() {
        assert_eq!(commify(0.0), "0");
        assert_eq!(commify(999.0), "999");
        assert_eq!(commify(1000.0), "1,000");
        assert_eq!(commify(-1234567.0), "-1,234,567");
        assert_eq!(commify(30512.345), "30,512.35");
    }

    #[test]
    fn uptime() {
        assert_eq!(fmt_uptime(0.0), "0+00:00:00");
        assert_eq!(fmt_uptime(126_780.0), "1+11:13:00");
    }

    #[test]
    fn zero_status_does_not_panic_or_nan() {
        let cur = map(&[("Uptime", "0"), ("Questions", "0")]);
        let h = compute_header(&cur, Some(&cur), Some(1.0), true);
        assert_eq!(h.avg_qps, 0.0);
        assert_eq!(h.siud, Siud::default());
        let now = h.now.as_ref().unwrap();
        assert!(now.siud.select.is_finite());
        let lines = format_header(&h, "localhost", "8.4.0", "12:00:00", 80, false);
        assert!(lines.iter().all(|l| !l.contains("NaN") && !l.contains("inf")));
    }

    #[test]
    fn deltas() {
        let old = map(&[
            ("Uptime", "100"),
            ("Questions", "1000"),
            ("Com_select", "500"),
            ("Com_insert", "100"),
            ("Bytes_received", "1000"),
        ]);
        let cur = map(&[
            ("Uptime", "110"),
            ("Questions", "1100"),
            ("Com_select", "550"),
            ("Com_insert", "125"),
            ("Bytes_received", "3000"),
            ("Key_read_requests", "100"),
            ("Key_reads", "1"),
        ]);
        let h = compute_header(&cur, Some(&old), Some(10.0), false);
        let now = h.now.unwrap();
        assert_eq!(now.qps, 10.0);
        assert_eq!(now.siud.select, 50.0);
        assert_eq!(now.siud.insert, 25.0);
        assert_eq!(now.bytes_in, 200.0);
        assert_eq!(h.key_efficiency, 99.0);
        assert!(h.qcache.is_none());
    }

    #[test]
    fn query_cache_line() {
        let cur = map(&[
            ("Uptime", "10"),
            ("Questions", "100"),
            ("Com_select", "30"),
            ("Qcache_hits", "10"),
        ]);
        let h = compute_header(&cur, None, None, true);
        let q = h.qcache.clone().unwrap();
        assert_eq!(q.ratio, 25.0);
        assert_eq!(h.siud.select, 40.0);
        let lines = format_header(&h, "h", "v", "00:00:00", 80, false);
        assert_eq!(lines.len(), 5);
        assert!(lines[3].starts_with(" Cache Hits:"));
    }
}
