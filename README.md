# rutop

A Rust rewrite of [mytop](https://github.com/jzawodn/mytop) 1.7 by Jeremy D. Zawodny: a `top`-like
monitor for MySQL / MariaDB. Single static binary for Linux and Windows, no Perl/DBI needed.

## Build

The release binaries are cross-compiled in Docker:

```powershell
.\build.ps1        # Windows
./build.sh         # Linux / macOS
```

Output:

| File | Target |
|---|---|
| `dist/rutop-linux-x86_64` | `x86_64-unknown-linux-musl` (static) |
| `dist/rutop-windows-x86_64.exe` | `x86_64-pc-windows-gnu` |

The Docker build also runs the unit tests. For local development, use `cargo build` / `cargo test`.

## Usage

```
rutop [options]
```

| Option | Description | Default |
|---|---|---|
| `-u`, `--user` | username | `root` |
| `-p`, `--pass`, `--password` | password | none |
| `--prompt` / `--noprompt` | prompt for the password (only if none is configured) | noprompt |
| `-h`, `--host host[:port]` | server host | `localhost` |
| `-P`, `--port` | server port | `3306` |
| `-S`, `--socket` | Unix socket (Windows: named pipe); overrides host/port if it exists | none |
| `-d`, `--db`, `--database` | database | `test` |
| `-s`, `--delay` | seconds between refreshes | `5` |
| `-b`, `--batch`, `--batchmode` | print once to stdout, no screen handling (`-m qps` prints forever) | off |
| `-m`, `--mode` | start mode: `top`, `qps`, `cmd`, `innodb`, `status` | `top` |
| `-i`, `--idle` / `--noidle` | show / hide idle (sleeping) threads; `--noidle` also reverses the sort order | idle |
| `-r`, `--resolve` / `--noresolve` | resolve client IPs to hostnames | noresolve |
| `--color` / `--nocolor` | colors | color |
| `--header` / `--noheader` | header display | header |
| `--long` / `--nolong` | long numbers (`1,234,567`) instead of short ones (`1.2M`) | short |
| `--sort 0\|1` | 1 = reverse order (longest running first) | 0 |
| `--ssl`, `--ssl-insecure` | use TLS (verify the certificate / don't verify it) | off |
| `-?`, `--help` | help | |

Negated options can also be written as `--no-color`, `--no-header`, and so on.

## Configuration files

Settings are applied in this order, each overriding the previous:

1. Defaults
2. `my.cnf`, groups `[client]` and `[mytop]` (keys: `user`, `password`, `host`, `port`, `socket`, `database`).
   On Unix these are `/etc/my.cnf`, `/etc/mysql/my.cnf` and `~/.my.cnf`; on Windows `%PROGRAMDATA%\MySQL\my.ini`,
   `%WINDIR%\my.ini`, `C:\my.ini` and `%USERPROFILE%\.my.cnf`.
3. `~/.mytop` (Windows: `%USERPROFILE%\.mytop`, or `%HOME%\.mytop` if `HOME` is set), in the same format as mytop:

   ```
   user=root
   pass=
   host=localhost
   db=test
   delay=5
   port=3306
   socket=
   batchmode=0
   header=1
   color=1
   idle=1
   # also: mode, sort, resolve, long_nums, prompt, filter_user, filter_db, filter_host (regex)
   ```
4. Command line

## Keys

| Key | Action |
|---|---|
| `?` | help |
| `t` | thread view (default) |
| `m` | queries-per-second view |
| `c` | command summary (`Com_*` counters) |
| `S` | status counters with changes (with idle off, only changed counters are shown) |
| `I` | InnoDB status |
| `V` | server variables |
| `d` / `u` / `h` | filter by database / user / host (blank = all, `/regex/` = regex, anything else = exact match) |
| `F` | remove all filters |
| `i` | toggle idle (sleeping) threads; also flips the sort order |
| `o` | reverse the sort order |
| `H` | toggle the header |
| `R` | toggle IP resolving |
| `L` | toggle short/long numbers |
| `s` | change the refresh delay |
| `p` | pause |
| `f` | show the full query of a thread (then `e` = explain) |
| `e` | EXPLAIN the query of a thread |
| `k` | kill a thread |
| `K` | kill all threads of a user |
| `r` | reset the server's status counters (`FLUSH STATUS`) |
| `D` | show the configuration |
| `#` | debug info (last query error, refresh time) |
| `↑ ↓ PgUp PgDn Home End` | scroll |
| `q` | quit (in the other views: back to the thread view) |
| `Esc` | back to the thread view |
| `Ctrl-C` | quit |

## Differences from mytop 1.7

- The TUI (ratatui) looks different. It has a status bar, popups instead of the external pager (`less`), scrolling,
  and a sparkline in qps mode. Colors and screen clearing work on Windows.
- Fixed:
  - `SHOW ENGINE INNODB STATUS`, with a fallback to the old syntax
  - qps mode uses the *global* `Questions` counter
  - no NaN or division-by-zero crashes with fresh or flushed counters
  - `EXPLAIN` shows all columns and reports errors
  - `USE db` is quoted
  - exact filters are literal, so `a.b` does not match `axb`
  - the command summary has no bogus deltas on the first refresh
  - failed reverse DNS lookups keep the IP
  - `FLUSH STATUS` resets the local deltas
  - the connection error message masks the password
- `q` in a secondary view goes back to the thread view, as the original's "[hit q to exit this mode]" message suggests.
- The thread line of the header ("qps now / Threads") is always shown. Before the second sample, the "now" values are `-`.
- The resolve default is *off*, as mytop's documentation says (the 1.7 code turned it on).
- New: `--noidle`, `--ssl`, `L`, scrolling.

## License

GPL-2.0, like mytop.
