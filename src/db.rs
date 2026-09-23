//! Thin wrapper around the MySQL connection (mytop's `Hashes`/`Execute`).

use std::path::Path;
use std::time::Duration;

use mysql::prelude::Queryable;
use mysql::{Conn, OptsBuilder, SslOpts, Value};

use crate::config::Config;

/// One result row; columns keep the server's order, NULL is `None`.
#[derive(Clone, Debug, Default)]
pub struct Row {
    cols: Vec<(String, Option<String>)>,
}

impl Row {
    #[cfg(test)]
    pub fn from_pairs(pairs: &[(&str, Option<&str>)]) -> Row {
        Row {
            cols: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.map(str::to_string)))
                .collect(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.cols
            .iter()
            .find(|(k, _)| k == name)
            .or_else(|| self.cols.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)))
            .and_then(|(_, v)| v.as_deref())
    }

    /// Value or empty string (Perl's `||= ''`).
    pub fn str(&self, name: &str) -> &str {
        self.get(name).unwrap_or("")
    }

    pub fn columns(&self) -> &[(String, Option<String>)] {
        &self.cols
    }
}

pub struct Db {
    conn: Conn,
}

/// Errors that mean the connection is gone (mytop exits in that case).
pub fn is_fatal(err: &mysql::Error) -> bool {
    matches!(
        err,
        mysql::Error::IoError(_) | mysql::Error::DriverError(_) | mysql::Error::CodecError(_)
    )
}

/// The socket to use, if any: it must exist (and be a socket on Unix).
fn usable_socket(cfg: &Config) -> Option<&str> {
    if cfg.socket.is_empty() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        match std::fs::metadata(Path::new(&cfg.socket)) {
            Ok(m) if m.file_type().is_socket() => Some(&cfg.socket),
            _ => None,
        }
    }
    #[cfg(not(unix))]
    {
        // Windows: named pipe name, e.g. "MySQL" or \\.\pipe\MySQL
        let _ = Path::new(&cfg.socket);
        Some(&cfg.socket)
    }
}

impl Db {
    pub fn connect(cfg: &Config) -> Result<Db, mysql::Error> {
        let mut opts = OptsBuilder::new()
            .user(Some(&cfg.user))
            .pass(if cfg.pass.is_empty() { None } else { Some(&cfg.pass) })
            .db_name(if cfg.db.is_empty() { None } else { Some(&cfg.db) })
            .tcp_connect_timeout(Some(Duration::from_secs(10)));

        opts = match usable_socket(cfg) {
            Some(sock) => opts.socket(Some(sock)),
            None => opts.ip_or_hostname(Some(&cfg.host)).tcp_port(cfg.port),
        };

        if cfg.ssl {
            let ssl = SslOpts::default()
                .with_danger_accept_invalid_certs(cfg.ssl_insecure)
                .with_danger_skip_domain_validation(cfg.ssl_insecure);
            opts = opts.ssl_opts(Some(ssl));
        }

        Ok(Db {
            conn: Conn::new(opts)?,
        })
    }

    /// Run a query and return all rows of all result sets.
    pub fn hashes(&mut self, sql: &str) -> Result<Vec<Row>, mysql::Error> {
        let mut out = Vec::new();
        let mut result = self.conn.query_iter(sql)?;
        while let Some(set) = result.iter() {
            for row in set {
                let row = row?;
                let names: Vec<String> = row
                    .columns_ref()
                    .iter()
                    .map(|c| c.name_str().into_owned())
                    .collect();
                let values = row.unwrap();
                out.push(Row {
                    cols: names
                        .into_iter()
                        .zip(values.into_iter().map(value_to_string))
                        .collect(),
                });
            }
        }
        Ok(out)
    }

    pub fn execute(&mut self, sql: &str) -> Result<(), mysql::Error> {
        self.conn.query_drop(sql)
    }
}

fn value_to_string(v: Value) -> Option<String> {
    Some(match v {
        Value::NULL => return None,
        Value::Bytes(b) => String::from_utf8_lossy(&b).into_owned(),
        Value::Int(i) => i.to_string(),
        Value::UInt(u) => u.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Double(d) => d.to_string(),
        Value::Date(y, m, d, h, mi, s, us) => {
            if us > 0 {
                format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}.{us:06}")
            } else {
                format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}")
            }
        }
        Value::Time(neg, d, h, m, s, us) => {
            let sign = if neg { "-" } else { "" };
            let hours = d * 24 + h as u32;
            if us > 0 {
                format!("{sign}{hours:02}:{m:02}:{s:02}.{us:06}")
            } else {
                format!("{sign}{hours:02}:{m:02}:{s:02}")
            }
        }
    })
}
