//! Core state shared by the TUI and batch mode: talks to the server and keeps
//! the data of all modes.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Instant;

use crate::config::{Config, Mode};
use crate::db::{self, Db};
use crate::filter::Filters;
use crate::modes::{CmdRow, CmdSummary, Qps, StatusRow, StatusView};
use crate::stats::{self, Header, StatusMap};
use crate::threads::{self, first_label, Thread};

/// Minimum interval between two status samples used for "now" rates.
const MIN_SAMPLE_SECS: f64 = 0.9;

pub enum RefreshError {
    /// Connection lost: mytop exits.
    Fatal(String),
    /// Query failed; show the error and carry on.
    Query(String),
}

fn wrap_err(e: mysql::Error) -> RefreshError {
    if db::is_fatal(&e) {
        RefreshError::Fatal(e.to_string())
    } else {
        RefreshError::Query(e.to_string())
    }
}

pub struct Monitor {
    pub cfg: Config,
    db: Db,
    pub version: String,
    pub have_query_cache: bool,
    pub filters: Filters,
    pub debug: bool,

    status: StatusMap,
    old_status: Option<StatusMap>,
    last_time: Option<Instant>,
    pub header: Option<Header>,

    /// Threads from the last processlist (unfiltered) - also used as the
    /// query/user/db cache for full query info, explain and kill.
    pub threads: Vec<Thread>,
    dns: HashMap<IpAddr, Option<String>>,

    cmd: CmdSummary,
    pub cmd_rows: Vec<CmdRow>,
    statview: StatusView,
    pub status_rows: Vec<StatusRow>,
    pub qps: Qps,
    pub innodb: String,
}

impl Monitor {
    pub fn new(cfg: Config, mut db: Db) -> Result<Monitor, mysql::Error> {
        let mut version = String::new();
        let mut have_query_cache = false;
        for row in db.hashes("SHOW VARIABLES")? {
            match row.str("Variable_name") {
                "version" => version = row.str("Value").to_string(),
                "have_query_cache" => have_query_cache = row.str("Value") == "YES",
                _ => {}
            }
        }
        let filters = Filters {
            user: cfg.filter_user.clone(),
            db: cfg.filter_db.clone(),
            host: cfg.filter_host.clone(),
        };
        Ok(Monitor {
            cfg,
            db,
            version,
            have_query_cache,
            filters,
            debug: false,
            status: StatusMap::new(),
            old_status: None,
            last_time: None,
            header: None,
            threads: Vec::new(),
            dns: HashMap::new(),
            cmd: CmdSummary::default(),
            cmd_rows: Vec::new(),
            statview: StatusView::default(),
            status_rows: Vec::new(),
            qps: Qps::default(),
            innodb: String::new(),
        })
    }

    pub fn refresh(&mut self, mode: Mode) -> Result<(), RefreshError> {
        match mode {
            Mode::Top => self.refresh_top(),
            Mode::Qps => self.refresh_qps().map(|_| ()),
            Mode::Cmd => self.refresh_cmd(),
            Mode::Status => self.refresh_status(),
            Mode::Innodb => self.refresh_innodb(),
        }
    }

    fn refresh_top(&mut self) -> Result<(), RefreshError> {
        if self.cfg.header {
            let rows = self.db.hashes("SHOW GLOBAL STATUS").map_err(wrap_err)?;
            if rows.is_empty() {
                return Err(RefreshError::Fatal("SHOW GLOBAL STATUS returned nothing".into()));
            }
            let now = Instant::now();
            let t_delta = self.last_time.map(|t| now.duration_since(t).as_secs_f64());
            let cur: StatusMap = rows
                .iter()
                .map(|r| (r.str("Variable_name").to_string(), r.str("Value").to_string()))
                .collect();

            // A refresh right after a key press: the interval is too short for
            // meaningful rates, so update the totals but keep the previous
            // "now" values and the sample baseline.
            if let (Some(dt), Some(prev)) = (t_delta, &self.header) {
                if dt < MIN_SAMPLE_SECS && !self.status.is_empty() {
                    let mut h = stats::compute_header(&cur, None, None, self.have_query_cache);
                    h.now = prev.now.clone();
                    if let (Some(q), Some(pq)) = (h.qcache.as_mut(), prev.qcache.as_ref()) {
                        q.hits_now = pq.hits_now;
                        q.ratio_now = pq.ratio_now;
                    }
                    self.header = Some(h);
                    return self.refresh_processlist();
                }
            }

            self.last_time = Some(now);
            let old = std::mem::replace(&mut self.status, cur);
            self.old_status = if old.is_empty() { None } else { Some(old) };
            self.header = Some(stats::compute_header(
                &self.status,
                self.old_status.as_ref(),
                t_delta,
                self.have_query_cache,
            ));
        }
        self.refresh_processlist()
    }

    fn refresh_processlist(&mut self) -> Result<(), RefreshError> {
        let rows = self.db.hashes("SHOW FULL PROCESSLIST").map_err(wrap_err)?;
        let mut threads: Vec<Thread> = rows.iter().map(Thread::from_row).collect();
        if self.cfg.resolve {
            for t in &mut threads {
                if let Some(ip) = t.ip {
                    if let Some(name) = self.resolve(ip) {
                        t.host = name;
                    }
                }
            }
        }
        self.threads = threads;
        Ok(())
    }

    fn resolve(&mut self, ip: IpAddr) -> Option<String> {
        self.dns
            .entry(ip)
            .or_insert_with(|| {
                dns_lookup::lookup_addr(&ip)
                    .ok()
                    .filter(|n| !n.is_empty() && n.parse::<IpAddr>().is_err())
                    .map(|n| first_label(&n))
            })
            .clone()
    }

    /// Returns the queries of the last second (None on the first sample).
    pub fn refresh_qps(&mut self) -> Result<Option<u64>, RefreshError> {
        let rows = self
            .db
            .hashes("SHOW GLOBAL STATUS LIKE 'Questions'")
            .map_err(wrap_err)?;
        let questions = rows
            .first()
            .and_then(|r| r.str("Value").trim().parse().ok())
            .unwrap_or(0);
        Ok(self.qps.update(questions))
    }

    fn refresh_cmd(&mut self) -> Result<(), RefreshError> {
        let rows = self
            .db
            .hashes("SHOW GLOBAL STATUS LIKE 'Com\\_%'")
            .map_err(wrap_err)?;
        self.cmd_rows = self.cmd.update(&rows);
        Ok(())
    }

    fn refresh_status(&mut self) -> Result<(), RefreshError> {
        let rows = self.db.hashes("SHOW GLOBAL STATUS").map_err(wrap_err)?;
        self.status_rows = self.statview.update(&rows, !self.cfg.idle);
        Ok(())
    }

    fn refresh_innodb(&mut self) -> Result<(), RefreshError> {
        let rows = match self.db.hashes("SHOW ENGINE INNODB STATUS") {
            Ok(r) => r,
            Err(e) if db::is_fatal(&e) => return Err(wrap_err(e)),
            // very old servers only know the old syntax
            Err(_) => self.db.hashes("SHOW INNODB STATUS").map_err(wrap_err)?,
        };
        self.innodb = rows
            .first()
            .map(|r| r.str("Status").to_string())
            .unwrap_or_default();
        Ok(())
    }

    pub fn visible_threads(&self) -> Vec<&Thread> {
        threads::visible(&self.threads, self.cfg.idle, self.cfg.sort, &self.filters)
    }

    pub fn header_lines(&self, width: usize) -> Vec<String> {
        match &self.header {
            Some(h) => {
                let clock = chrono::Local::now().format("%H:%M:%S").to_string();
                stats::format_header(
                    h,
                    &self.cfg.host,
                    &self.version,
                    &clock,
                    width,
                    self.cfg.long_nums,
                )
            }
            None => Vec::new(),
        }
    }

    pub fn thread(&self, id: u64) -> Option<&Thread> {
        self.threads.iter().find(|t| t.id == id)
    }

    /// `SHOW VARIABLES` formatted like mytop's `V` view.
    pub fn variables(&mut self) -> Result<Vec<String>, RefreshError> {
        let rows = self.db.hashes("SHOW VARIABLES").map_err(wrap_err)?;
        Ok(rows
            .iter()
            .map(|r| format!("{:>32}: {}", r.str("Variable_name"), r.str("Value")))
            .collect())
    }

    /// EXPLAIN the query of a thread; returns printable lines.
    pub fn explain(&mut self, id: u64) -> Result<Vec<String>, String> {
        let Some(t) = self.thread(id) else {
            return Err("*** Invalid id. ***".into());
        };
        if t.info.is_empty() {
            return Err(format!("*** Thread {id} is not running a query. ***"));
        }
        let (sql, db) = (t.info.clone(), t.db.clone());
        if !db.is_empty() {
            self.db
                .execute(&format!("USE `{}`", db.replace('`', "``")))
                .map_err(|e| e.to_string())?;
        }
        let rows = self
            .db
            .hashes(&format!("EXPLAIN {sql}"))
            .map_err(|e| format!("EXPLAIN failed: {e}"))?;
        let mut out = vec![format!("EXPLAIN {sql}:"), String::new()];
        for (i, row) in rows.iter().enumerate() {
            out.push(format!("*** row {} ***", i + 1));
            for (k, v) in row.columns() {
                let v = v.as_deref().filter(|v| !v.is_empty()).unwrap_or("NULL");
                out.push(format!("{k:>15}:  {v}"));
            }
        }
        Ok(out)
    }

    pub fn kill(&mut self, id: u64) -> Result<(), String> {
        self.db
            .execute(&format!("KILL {id}"))
            .map_err(|e| e.to_string())
    }

    /// Kill all threads of a user (from the last processlist).
    /// Returns (killed, errors).
    pub fn kill_user(&mut self, user: &str) -> (usize, Vec<String>) {
        let ids: Vec<u64> = self
            .threads
            .iter()
            .filter(|t| t.user == user)
            .map(|t| t.id)
            .collect();
        let mut killed = 0;
        let mut errors = Vec::new();
        for id in ids {
            match self.kill(id) {
                Ok(()) => killed += 1,
                Err(e) => errors.push(format!("{id}: {e}")),
            }
        }
        (killed, errors)
    }

    /// FLUSH STATUS; also forget previous samples so no bogus deltas appear.
    pub fn flush_status(&mut self) -> Result<(), String> {
        self.db.execute("FLUSH STATUS").map_err(|e| e.to_string())?;
        self.old_status = None;
        self.status.clear();
        self.last_time = None;
        self.cmd.reset();
        self.statview.reset();
        self.qps.reset();
        Ok(())
    }
}
