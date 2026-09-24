//! Configuration: defaults, option files (`my.cnf` groups `[client]`/`[mytop]`,
//! `~/.mytop`) and command-line arguments, applied in that order.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{ArgAction, Parser, ValueEnum};

use crate::filter::Filter;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    Top,
    Qps,
    Cmd,
    Innodb,
    Status,
}

impl Mode {
    fn parse(s: &str) -> Option<Mode> {
        Mode::from_str(s.trim(), true).ok()
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Mode::Top => "top",
            Mode::Qps => "qps",
            Mode::Cmd => "cmd",
            Mode::Innodb => "innodb",
            Mode::Status => "status",
        };
        f.write_str(s)
    }
}

/// Transport to the server, like the mysql client's `--protocol`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Protocol {
    Tcp,
    Socket,
}

impl Protocol {
    fn parse(s: &str) -> Option<Protocol> {
        match s.trim().to_ascii_lowercase().as_str() {
            "tcp" => Some(Protocol::Tcp),
            // "pipe" is the Windows spelling of a local connection
            "socket" | "pipe" => Some(Protocol::Socket),
            _ => None,
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Protocol::Tcp => "tcp",
            Protocol::Socket => "socket",
        })
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub batchmode: bool,
    pub color: bool,
    pub db: String,
    pub delay: u64,
    pub filter_user: Filter,
    pub filter_db: Filter,
    pub filter_host: Filter,
    pub header: bool,
    pub host: String,
    pub idle: bool,
    pub long_nums: bool,
    pub mode: Mode,
    pub prompt: bool,
    pub pass: String,
    pub port: u16,
    /// Port given on the command line: implies TCP (like the MariaDB client).
    pub port_explicit: bool,
    /// None = automatic (local socket for localhost if one exists)
    pub protocol: Option<Protocol>,
    pub resolve: bool,
    pub socket: String,
    /// false = default order (least idle first), true = reversed
    pub sort: bool,
    pub user: String,
    pub ssl: bool,
    pub ssl_insecure: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            batchmode: false,
            color: true,
            db: "test".into(),
            delay: 5,
            filter_user: Filter::Any,
            filter_db: Filter::Any,
            filter_host: Filter::Any,
            header: true,
            host: "localhost".into(),
            idle: true,
            long_nums: false,
            mode: Mode::Top,
            prompt: false,
            pass: String::new(),
            port: 3306,
            port_explicit: false,
            protocol: None,
            resolve: false,
            socket: String::new(),
            sort: false,
            user: "root".into(),
            ssl: false,
            ssl_insecure: false,
        }
    }
}

impl Config {
    /// Human readable dump (password masked), used by the `D` key.
    pub fn dump(&self) -> Vec<String> {
        let masked = if self.pass.is_empty() { "" } else { "********" };
        vec![
            format!("{:>12} = {}", "user", self.user),
            format!("{:>12} = {}", "pass", masked),
            format!("{:>12} = {}", "host", self.host),
            format!("{:>12} = {}", "port", self.port),
            format!("{:>12} = {}", "socket", self.socket),
            format!("{:>12} = {}", "protocol", self.protocol.map_or("auto".to_string(), |p| p.to_string())),
            format!("{:>12} = {}", "db", self.db),
            format!("{:>12} = {}", "delay", self.delay),
            format!("{:>12} = {}", "mode", self.mode),
            format!("{:>12} = {}", "batchmode", self.batchmode as u8),
            format!("{:>12} = {}", "header", self.header as u8),
            format!("{:>12} = {}", "color", self.color as u8),
            format!("{:>12} = {}", "idle", self.idle as u8),
            format!("{:>12} = {}", "sort", self.sort as u8),
            format!("{:>12} = {}", "resolve", self.resolve as u8),
            format!("{:>12} = {}", "long_nums", self.long_nums as u8),
            format!("{:>12} = {}", "prompt", self.prompt as u8),
            format!("{:>12} = {}", "ssl", self.ssl as u8),
            format!("{:>12} = {}", "filter_user", self.filter_user),
            format!("{:>12} = {}", "filter_db", self.filter_db),
            format!("{:>12} = {}", "filter_host", self.filter_host),
        ]
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "rutop",
    version,
    about = "Display MySQL server performance info like `top' (Rust rewrite of mytop)",
    disable_help_flag = true
)]
struct Cli {
    /// Print help
    #[arg(short = '?', long = "help", action = ArgAction::Help)]
    help: Option<bool>,

    /// Username for the MySQL server [default: root]
    #[arg(short = 'u', long = "user")]
    user: Option<String>,

    /// Password for the MySQL server [default: none]
    #[arg(short = 'p', long = "pass", visible_alias = "password")]
    pass: Option<String>,

    /// Database to connect to [default: test]
    #[arg(short = 'd', long = "database", visible_alias = "db")]
    db: Option<String>,

    /// Hostname of the server, optionally followed by :port [default: localhost]
    #[arg(short = 'h', long = "host")]
    host: Option<String>,

    /// TCP port of the server; implies --protocol tcp [default: 3306]
    #[arg(short = 'P', long = "port")]
    port: Option<u16>,

    /// Unix socket (named pipe on Windows); takes precedence over host/port
    #[arg(short = 'S', long = "socket")]
    socket: Option<String>,

    /// Connection protocol [default: socket for localhost if one exists, else tcp]
    #[arg(long = "protocol", value_enum)]
    protocol: Option<Protocol>,

    /// Seconds between display refreshes [default: 5]
    #[arg(short = 's', long = "delay")]
    delay: Option<u64>,

    /// Batch mode: print once to stdout, no screen handling
    #[arg(short = 'b', long = "batch", visible_alias = "batchmode")]
    batch: bool,

    /// Show idle (sleeping) threads [default]
    #[arg(short = 'i', long = "idle", overrides_with = "noidle")]
    idle: bool,
    /// Hide idle (sleeping) threads; also reverses the sort order
    #[arg(long = "noidle", alias = "no-idle", overrides_with = "idle")]
    noidle: bool,

    /// Resolve IP addresses to hostnames
    #[arg(short = 'r', long = "resolve", overrides_with = "noresolve")]
    resolve: bool,
    /// Do not resolve IP addresses [default]
    #[arg(long = "noresolve", alias = "no-resolve", overrides_with = "resolve")]
    noresolve: bool,

    /// Use colors [default]
    #[arg(long = "color", overrides_with = "nocolor")]
    color: bool,
    /// Disable colors
    #[arg(long = "nocolor", alias = "no-color", overrides_with = "color")]
    nocolor: bool,

    /// Show the header [default]
    #[arg(long = "header", overrides_with = "noheader")]
    header: bool,
    /// Hide the header
    #[arg(long = "noheader", alias = "no-header", overrides_with = "header")]
    noheader: bool,

    /// Prompt for the password
    #[arg(long = "prompt", overrides_with = "noprompt")]
    prompt: bool,
    /// Do not prompt for the password [default]
    #[arg(long = "noprompt", alias = "no-prompt", overrides_with = "prompt")]
    noprompt: bool,

    /// Show long numbers (1,234,567) instead of short ones (1.2M)
    #[arg(long = "long", overrides_with = "nolong")]
    long: bool,
    /// Show short numbers [default]
    #[arg(long = "nolong", alias = "no-long", overrides_with = "long")]
    nolong: bool,

    /// Start mode
    #[arg(short = 'm', long = "mode", value_enum)]
    mode: Option<Mode>,

    /// Reverse sort order (1) or default order (0)
    #[arg(long = "sort")]
    sort: Option<String>,

    /// Use TLS for the connection (certificate is verified)
    #[arg(long = "ssl")]
    ssl: bool,
    /// Use TLS but do not verify the server certificate
    #[arg(long = "ssl-insecure")]
    ssl_insecure: bool,
}

fn flag(on: bool, off: bool) -> Option<bool> {
    match (on, off) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

/// Perl truthiness as used by mytop's config values ("" and "0" are false).
fn truthy(v: &str) -> bool {
    let v = v.trim();
    !(v.is_empty()
        || v == "0"
        || v.eq_ignore_ascii_case("no")
        || v.eq_ignore_ascii_case("false")
        || v.eq_ignore_ascii_case("off"))
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
}

/// Load the complete configuration.
pub fn load() -> Result<Config> {
    let mut cfg = Config::default();

    for path in mycnf_paths() {
        if let Ok(text) = fs::read_to_string(&path) {
            apply_mycnf(&mut cfg, &text);
        }
    }

    if let Some(home) = home_dir() {
        let path = home.join(".mytop");
        if path.exists() {
            let text = fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            apply_mytop(&mut cfg, &text)
                .with_context(|| format!("invalid config file {}", path.display()))?;
        }
    }

    split_host_port(&mut cfg);
    let cli = Cli::parse();
    apply_cli(&mut cfg, cli);
    if cfg.delay < 1 {
        cfg.delay = 1;
    }
    Ok(cfg)
}

fn mycnf_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if cfg!(windows) {
        if let Some(pd) = std::env::var_os("PROGRAMDATA") {
            paths.push(Path::new(&pd).join("MySQL").join("my.ini"));
        }
        if let Some(windir) = std::env::var_os("WINDIR") {
            paths.push(Path::new(&windir).join("my.ini"));
            paths.push(Path::new(&windir).join("my.cnf"));
        }
        paths.push(PathBuf::from(r"C:\my.ini"));
        paths.push(PathBuf::from(r"C:\my.cnf"));
    } else {
        paths.push(PathBuf::from("/etc/my.cnf"));
        paths.push(PathBuf::from("/etc/mysql/my.cnf"));
    }
    if let Some(home) = home_dir() {
        paths.push(home.join(".my.cnf"));
    }
    paths
}

/// Apply the `[client]` and `[mytop]` groups of a my.cnf style file.
pub fn apply_mycnf(cfg: &mut Config, text: &str) {
    let mut active = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') || line.starts_with('!')
        {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let group = line[1..line.len() - 1].trim().to_ascii_lowercase();
            active = group == "client" || group == "mytop";
            continue;
        }
        if !active {
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some((k, v)) => (k.trim(), unquote(v.trim())),
            None => continue,
        };
        match key.to_ascii_lowercase().replace('-', "_").as_str() {
            "user" => cfg.user = value,
            "password" | "pass" => cfg.pass = value,
            "host" => cfg.host = value,
            "port" => {
                if let Ok(p) = value.parse() {
                    cfg.port = p
                }
            }
            "socket" => cfg.socket = value,
            "protocol" => {
                if let Some(p) = Protocol::parse(&value) {
                    cfg.protocol = Some(p)
                }
            }
            "database" | "db" => cfg.db = value,
            _ => {}
        }
    }
}

fn unquote(v: &str) -> String {
    let b = v.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

/// Apply a `~/.mytop` file (`key=value` lines, `#` comments, unknown keys ignored).
pub fn apply_mytop(cfg: &mut Config, text: &str) -> Result<()> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        // like mytop (`(\S+)\s*=\s*(.*\S)`): empty values leave the setting alone
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "batchmode" => cfg.batchmode = truthy(value),
            "color" => cfg.color = truthy(value),
            "db" => cfg.db = value.to_string(),
            "delay" => {
                if let Ok(d) = value.parse() {
                    cfg.delay = d
                }
            }
            "filter_user" => cfg.filter_user = Filter::raw_regex(value)?,
            "filter_db" => cfg.filter_db = Filter::raw_regex(value)?,
            "filter_host" => cfg.filter_host = Filter::raw_regex(value)?,
            "header" => cfg.header = truthy(value),
            "host" => cfg.host = value.to_string(),
            "idle" => cfg.idle = truthy(value),
            "long_nums" => cfg.long_nums = truthy(value),
            "mode" => {
                if let Some(m) = Mode::parse(value) {
                    cfg.mode = m
                }
            }
            "prompt" => cfg.prompt = truthy(value),
            "pass" => cfg.pass = value.to_string(),
            "port" => {
                if let Ok(p) = value.parse() {
                    cfg.port = p
                }
            }
            "resolve" => cfg.resolve = truthy(value),
            "socket" => cfg.socket = value.to_string(),
            "sort" => cfg.sort = truthy(value),
            "user" => cfg.user = value.to_string(),
            _ => {}
        }
    }
    Ok(())
}

fn apply_cli(cfg: &mut Config, cli: Cli) {
    if let Some(v) = cli.user {
        cfg.user = v;
    }
    if let Some(v) = cli.pass {
        cfg.pass = v;
    }
    if let Some(v) = cli.db {
        cfg.db = v;
    }
    if let Some(v) = cli.port {
        cfg.port = v;
        cfg.port_explicit = true;
    }
    if let Some(v) = cli.host {
        cfg.host = v;
        if split_host_port(cfg) {
            cfg.port_explicit = true;
        }
    }
    if let Some(v) = cli.socket {
        cfg.socket = v;
    }
    if let Some(v) = cli.protocol {
        cfg.protocol = Some(v);
    }
    if let Some(v) = cli.delay {
        cfg.delay = v;
    }
    if cli.batch {
        cfg.batchmode = true;
    }
    if let Some(v) = flag(cli.idle, cli.noidle) {
        cfg.idle = v;
        // mytop: hiding idle threads reverses the order (longest running first)
        cfg.sort = !v;
    }
    if let Some(v) = flag(cli.resolve, cli.noresolve) {
        cfg.resolve = v;
    }
    if let Some(v) = flag(cli.color, cli.nocolor) {
        cfg.color = v;
    }
    if let Some(v) = flag(cli.header, cli.noheader) {
        cfg.header = v;
    }
    if let Some(v) = flag(cli.prompt, cli.noprompt) {
        cfg.prompt = v;
    }
    if let Some(v) = flag(cli.long, cli.nolong) {
        cfg.long_nums = v;
    }
    if let Some(v) = cli.mode {
        cfg.mode = v;
    }
    if let Some(v) = cli.sort {
        cfg.sort = truthy(&v);
    }
    if cli.ssl || cli.ssl_insecure {
        cfg.ssl = true;
    }
    cfg.ssl_insecure = cli.ssl_insecure;
}

/// The user may have put the port with the host (`host:port`).
/// Returns true if a port was split off.
fn split_host_port(cfg: &mut Config) -> bool {
    if let Some((host, port)) = cfg.host.rsplit_once(':') {
        if !host.contains(':') && !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(p) = port.parse() {
                cfg.port = p;
                cfg.host = host.to_string();
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mytop_file() {
        let mut cfg = Config::default();
        apply_mytop(
            &mut cfg,
            "# comment\n\nuser = bob\npass=\nHOST=db1\ndelay= 2\nidle=0\ncolor=0\nunknown=1\nmode=cmd\n",
        )
        .unwrap();
        assert_eq!(cfg.user, "bob");
        assert_eq!(cfg.host, "db1");
        assert_eq!(cfg.delay, 2);
        assert!(!cfg.idle);
        assert!(!cfg.color);
        assert_eq!(cfg.mode, Mode::Cmd);
    }

    #[test]
    fn mycnf_groups() {
        let mut cfg = Config::default();
        apply_mycnf(
            &mut cfg,
            "[mysqld]\nuser=mysql\n[client]\nuser=alice\npassword=\"se cret\"\n[mytop]\nport=3307\n",
        );
        assert_eq!(cfg.user, "alice");
        assert_eq!(cfg.pass, "se cret");
        assert_eq!(cfg.port, 3307);
    }

    #[test]
    fn host_with_port() {
        let mut cfg = Config {
            host: "db.example.com:3310".into(),
            ..Config::default()
        };
        split_host_port(&mut cfg);
        assert_eq!(cfg.host, "db.example.com");
        assert_eq!(cfg.port, 3310);

        let mut cfg = Config {
            host: "::1".into(),
            ..Config::default()
        };
        split_host_port(&mut cfg);
        assert_eq!(cfg.host, "::1");
        assert_eq!(cfg.port, 3306);
    }

    fn with_args(cfg: &mut Config, args: &[&str]) {
        let cli = Cli::try_parse_from(std::iter::once("rutop").chain(args.iter().copied())).unwrap();
        apply_cli(cfg, cli);
    }

    #[test]
    fn explicit_port_only_from_command_line() {
        let mut cfg = Config::default();
        apply_mycnf(&mut cfg, "[client]\nport=3307\nhost=db1:3308\nprotocol=TCP\n");
        split_host_port(&mut cfg);
        assert_eq!((cfg.host.as_str(), cfg.port), ("db1", 3308));
        assert_eq!(cfg.protocol, Some(Protocol::Tcp));
        assert!(!cfg.port_explicit);

        let mut cfg = Config::default();
        with_args(&mut cfg, &["-P", "3307"]);
        assert!(cfg.port_explicit);

        let mut cfg = Config::default();
        with_args(&mut cfg, &["-h", "localhost:3309"]);
        assert_eq!((cfg.host.as_str(), cfg.port), ("localhost", 3309));
        assert!(cfg.port_explicit);

        let mut cfg = Config::default();
        with_args(&mut cfg, &["-h", "db2", "--protocol", "socket"]);
        assert!(!cfg.port_explicit);
        assert_eq!(cfg.protocol, Some(Protocol::Socket));
    }
}
