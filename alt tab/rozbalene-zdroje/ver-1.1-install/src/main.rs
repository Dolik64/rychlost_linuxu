mod app;
mod mru;
mod icons;
mod ui;
mod wayland;

use anyhow::Result;
use clap::Parser;
use std::io::Write;
use std::os::{fd::AsRawFd, unix::fs::{OpenOptionsExt, PermissionsExt}};
use std::sync::OnceLock;
use std::os::unix::net::UnixListener;
pub static UI_LISTENER: OnceLock<UnixListener> = OnceLock::new();

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

pub struct SocketCleanup(pub std::path::PathBuf);
impl Drop for SocketCleanup {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
}

use std::os::unix::net::UnixStream;
use cosmic::cosmic_config::ConfigGet;
use switcher_config::{Theme, WorkspaceScope, APP_ID, CONFIG_VERSION};

pub fn socket_path() -> std::path::PathBuf {
    runtime_dir().expect("runtime checked in main").join("cosmic-ext-app-switcher.sock")
}

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

    // Serialize startup so rapid shortcut invocations cannot open two overlays.
    let startup_lock = file_lock("cosmic-ext-app-switcher-ui.lock", false)?;
    let cmd = if args.reverse { b"prev" as &[u8] } else { b"next" as &[u8] };
    if let Ok(mut stream) = UnixStream::connect(socket_path()) {
        stream.set_write_timeout(Some(std::time::Duration::from_secs(1)))?;
        stream.write_all(cmd)?;
        return Ok(());
    }
    let _ = std::fs::remove_file(socket_path());
    let listener = UnixListener::bind(socket_path())?;
    std::fs::set_permissions(socket_path(), std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    UI_LISTENER.set(listener).map_err(|_| anyhow::anyhow!("UI socket already initialized"))?;
    let _cleanup = SocketCleanup(socket_path());
    drop(startup_lock);

    let snapshot = wayland::snapshot(load_scope())?;
    if snapshot.toplevels.is_empty() { return Ok(()); }
    app::run(snapshot.toplevels, args.reverse, snapshot.generation, load_theme())
}
