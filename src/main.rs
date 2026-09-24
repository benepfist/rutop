//! rutop - display MySQL server performance info like `top'.
//! A Rust rewrite of mytop by Jeremy D. Zawodny.

mod batch;
mod config;
mod db;
mod filter;
mod modes;
mod monitor;
mod stats;
mod text;
mod threads;
mod tui;

use std::process::ExitCode;

use config::Config;
use db::Db;
use monitor::Monitor;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rutop: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let mut cfg = config::load()?;

    lower_priority();

    if cfg.prompt && cfg.pass.is_empty() {
        cfg.pass = rpassword::prompt_password("Password: ")?;
    }

    let transport = db::transport(&cfg).map_err(|e| anyhow::anyhow!("Cannot connect to MySQL server: {e}"))?;
    let db = match Db::connect(&cfg, &transport) {
        Ok(db) => db,
        Err(e) => anyhow::bail!("{}", connect_error(&cfg, &transport, &e)),
    };
    let mon = Monitor::new(cfg, db)?;

    if mon.cfg.batchmode {
        batch::run(mon)
    } else {
        tui::run(mon)
    }
}

fn connect_error(cfg: &Config, transport: &db::Transport, err: &mysql::Error) -> String {
    let pass = if cfg.pass.is_empty() { "" } else { "********" };
    format!(
        r#"Cannot connect to MySQL server via {}. Please check the:

  * database you specified "{}" (default is "test")
  * username you specified "{}" (default is "root")
  * password you specified "{}" (default is "")
  * hostname you specified "{}" (default is "localhost")
  * port you specified "{}" (default is 3306)
  * socket you specified "{}" (default is "")

For localhost the local server socket is used if one exists; a port on the
command line or --protocol tcp connects via TCP instead.

The options may be specified on the command-line or in a ~/.mytop
config file. See `rutop --help` and the README for details.

Here's the exact error from the MySQL driver. It might help you debug:

{}
"#,
        transport, cfg.db, cfg.user, pass, cfg.host, cfg.port, cfg.socket, err
    )
}

/// Try to lower our priority (like mytop's `setpriority(0,0,10)`).
fn lower_priority() {
    #[cfg(unix)]
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
}
