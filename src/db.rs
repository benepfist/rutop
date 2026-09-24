//! Thin wrapper around the MySQL connection (mytop's `Hashes`/`Execute`).

use std::fmt;
use std::time::Duration;

use mysql::prelude::Queryable;
use mysql::{Conn, OptsBuilder, SslOpts, Value};

use crate::config::{Config, Protocol};

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

/// How to reach the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transport {
    /// Unix socket (named pipe on Windows)
    Socket(String),
    Tcp(String, u16),
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Transport::Socket(s) => write!(f, "socket {s}"),
            Transport::Tcp(h, p) => write!(f, "TCP {h}:{p}"),
        }
    }
}

/// Where MySQL/MariaDB packages put the server socket.
#[cfg(unix)]
const DEFAULT_SOCKETS: &[&str] = &[
    "/var/lib/mysql/mysql.sock",
    "/run/mysqld/mysqld.sock",
    "/var/run/mysqld/mysqld.sock",
    "/tmp/mysql.sock",
];

/// Choose the transport like the mysql/mariadb client: `localhost` means the
/// local socket unless TCP is asked for (`--protocol tcp` or a port on the
/// command line).
pub fn transport(cfg: &Config) -> Result<Transport, String> {
    #[cfg(unix)]
    {
        let env = std::env::var("MYSQL_UNIX_PORT").ok().filter(|s| !s.is_empty());
        pick(cfg, env.as_deref(), DEFAULT_SOCKETS, is_socket)
    }
    #[cfg(not(unix))]
    {
        pick(cfg, None, &[], is_socket)
    }
}

/// Is this a usable socket? It must exist and be a socket on Unix.
fn is_socket(path: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        std::fs::metadata(path).is_ok_and(|m| m.file_type().is_socket())
    }
    #[cfg(not(unix))]
    {
        // Windows: named pipe name, e.g. "MySQL" or \\.\pipe\MySQL
        !path.is_empty()
    }
}

fn pick(
    cfg: &Config,
    env_socket: Option<&str>,
    defaults: &[&str],
    is_socket: impl Fn(&str) -> bool,
) -> Result<Transport, String> {
    let local = cfg.host.is_empty() || cfg.host.eq_ignore_ascii_case("localhost");
    // localhost may resolve to ::1 while the server only listens on IPv4
    let tcp = || {
        let host = if local { "127.0.0.1" } else { cfg.host.as_str() };
        Transport::Tcp(host.to_string(), cfg.port)
    };
    let default_socket = || {
        env_socket
            .into_iter()
            .chain(defaults.iter().copied())
            .find(|s| is_socket(s))
            .map(|s| Transport::Socket(s.to_string()))
    };

    match cfg.protocol {
        Some(Protocol::Tcp) => Ok(tcp()),
        Some(Protocol::Socket) if !cfg.socket.is_empty() => Ok(Transport::Socket(cfg.socket.clone())),
        Some(Protocol::Socket) => {
            default_socket().ok_or_else(|| "no local server socket found, use -S to name it".into())
        }
        None if !cfg.socket.is_empty() && is_socket(&cfg.socket) => {
            Ok(Transport::Socket(cfg.socket.clone()))
        }
        None if local && !cfg.port_explicit => Ok(default_socket().unwrap_or_else(tcp)),
        None => Ok(tcp()),
    }
}

impl Db {
    pub fn connect(cfg: &Config, transport: &Transport) -> Result<Db, mysql::Error> {
        let mut opts = OptsBuilder::new()
            .user(Some(&cfg.user))
            .pass(if cfg.pass.is_empty() { None } else { Some(&cfg.pass) })
            .db_name(if cfg.db.is_empty() { None } else { Some(&cfg.db) })
            .tcp_connect_timeout(Some(Duration::from_secs(10)));

        opts = match transport {
            Transport::Socket(sock) => opts.socket(Some(sock)),
            Transport::Tcp(host, port) => opts.ip_or_hostname(Some(host)).tcp_port(*port),
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

#[cfg(test)]
mod tests {
    use super::*;

    const SOCK: &str = "/var/lib/mysql/mysql.sock";
    const DEFAULTS: &[&str] = &["/run/mysqld/mysqld.sock", SOCK];

    fn pick_with(cfg: &Config, env: Option<&str>, present: &[&str]) -> Result<Transport, String> {
        pick(cfg, env, DEFAULTS, |s| present.contains(&s))
    }

    fn tcp(host: &str, port: u16) -> Result<Transport, String> {
        Ok(Transport::Tcp(host.into(), port))
    }

    fn sock(path: &str) -> Result<Transport, String> {
        Ok(Transport::Socket(path.into()))
    }

    #[test]
    fn localhost_prefers_default_socket() {
        let cfg = Config::default();
        assert_eq!(pick_with(&cfg, None, &[SOCK]), sock(SOCK));
        assert_eq!(pick_with(&cfg, None, &[]), tcp("127.0.0.1", 3306));
        assert_eq!(
            pick_with(&cfg, Some("/srv/my.sock"), &[SOCK, "/srv/my.sock"]),
            sock("/srv/my.sock")
        );
        // a port from an option file does not force TCP
        let cfg = Config {
            port: 3307,
            ..Config::default()
        };
        assert_eq!(pick_with(&cfg, None, &[SOCK]), sock(SOCK));
    }

    #[test]
    fn command_line_port_forces_tcp() {
        let cfg = Config {
            port: 3307,
            port_explicit: true,
            ..Config::default()
        };
        assert_eq!(pick_with(&cfg, None, &[SOCK]), tcp("127.0.0.1", 3307));
    }

    #[test]
    fn protocol_option() {
        let cfg = Config {
            protocol: Some(Protocol::Tcp),
            ..Config::default()
        };
        assert_eq!(pick_with(&cfg, None, &[SOCK]), tcp("127.0.0.1", 3306));

        let cfg = Config {
            protocol: Some(Protocol::Socket),
            port_explicit: true,
            ..Config::default()
        };
        assert_eq!(pick_with(&cfg, None, &[SOCK]), sock(SOCK));
        assert!(pick_with(&cfg, None, &[]).is_err());
    }

    #[test]
    fn remote_host_and_explicit_socket() {
        let cfg = Config {
            host: "db1".into(),
            ..Config::default()
        };
        assert_eq!(pick_with(&cfg, None, &[SOCK]), tcp("db1", 3306));

        let cfg = Config {
            host: "db1".into(),
            socket: "/tmp/x.sock".into(),
            ..Config::default()
        };
        assert_eq!(pick_with(&cfg, None, &["/tmp/x.sock"]), sock("/tmp/x.sock"));
        // missing socket falls back like mytop
        assert_eq!(pick_with(&cfg, None, &[]), tcp("db1", 3306));
    }
}
