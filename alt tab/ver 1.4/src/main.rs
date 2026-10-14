mod app;
mod ipc;
mod lifecycle;
mod mru;
mod icons;
mod ui;
mod wayland;

use anyhow::Result;
use clap::Parser;
use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};

pub fn runtime_dir() -> Result<std::path::PathBuf> {
    let path = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("Run inside your logged-in COSMIC desktop (XDG_RUNTIME_DIR is missing)"))?;
    Ok(path)
}

pub fn file_lock(name: &str, nonblocking: bool) -> Result<std::fs::File> {
    let file = std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true)
        .mode(0o600).open(runtime_dir()?.join(name))?;
    let operation = libc::LOCK_EX | if nonblocking { libc::LOCK_NB } else { 0 };
    if unsafe { libc::flock(file.as_raw_fd(), operation) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(file)
}

use cosmic::cosmic_config::ConfigGet;
use switcher_config::{Theme, WorkspaceScope, APP_ID, CONFIG_VERSION};

#[derive(Parser, Debug)]
#[command(name = "cosmic-ext-app-switcher", version)]
pub struct Args {
    #[arg(long, default_value_t = false)]
    pub reverse: bool,
    /// Track window activation history continuously (run via systemd --user).
    #[arg(long)]
    pub daemon: bool,
    /// Check that the history service is reachable.
    #[arg(long)]
    pub mru_check: bool,
    /// Print current MRU order as JSON without opening the switcher.
    #[arg(long)]
    pub mru_list: bool,

    /// Report whether the compositor offers the Wayland interfaces the switcher needs, then exit
    #[arg(long, default_value_t = false)]
    pub check_compat: bool,
}

fn load_theme() -> Theme {
    use cosmic::cosmic_config::Config;
    Config::new(APP_ID, CONFIG_VERSION)
        .ok()
        .and_then(|c| c.get::<Theme>("theme").ok())
        .unwrap_or_default()
}

fn load_scope() -> WorkspaceScope {
    use cosmic::cosmic_config::Config;
    Config::new(APP_ID, CONFIG_VERSION)
        .ok()
        .and_then(|c| c.get::<WorkspaceScope>("workspace_scope").ok())
        .unwrap_or_default()
}

/// Prints one line per required Wayland global. Exits with status 1 if any is unusable.
fn check_compat() -> Result<()> {
    let statuses = wayland::probe_required_globals()?;
    for status in &statuses {
        println!("{status}");
    }
    if !statuses.iter().all(wayland::GlobalStatus::ok) {
        std::process::exit(1);
    }
    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    runtime_dir()?;
    if args.daemon { return wayland::run_daemon(); }
    if args.mru_check { wayland::ping()?; println!("MRU service OK"); return Ok(()); }
    if args.mru_list {
        println!("{}", serde_json::to_string_pretty(&wayland::snapshot(load_scope())?)?);
        return Ok(());
    }
    if args.check_compat {
        return check_compat();
    }

    let Some(owner) = ipc::claim_or_forward(&runtime_dir()?, args.reverse)? else {
        return Ok(());
    };
    let snapshot = wayland::snapshot(load_scope())?;
    if snapshot.toplevels.is_empty() { return Ok(()); }
    // Keep ownership through the full event loop and its renderer teardown.
    let result = app::run(snapshot.toplevels, args.reverse, snapshot.generation,
        load_theme(), owner.listener());
    drop(owner);
    result
}
