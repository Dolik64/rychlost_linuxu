use std::{
    io::{Read, Write},
    os::{fd::AsRawFd, unix::{fs::PermissionsExt, net::{UnixListener, UnixStream}}},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use serde::{Deserialize, Serialize};
use crate::mru::History;

use anyhow::{Context, Result, anyhow};
use wayland_client::{
    Connection, Dispatch, QueueHandle, event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_output, wl_registry, wl_seat},
};
use cosmic_protocols::{
    toplevel_info::v1::client::{
        zcosmic_toplevel_handle_v1::{self, ZcosmicToplevelHandleV1},
        zcosmic_toplevel_info_v1::{self, ZcosmicToplevelInfoV1},
    },
    toplevel_management::v1::client::zcosmic_toplevel_manager_v1::{
        self, ZcosmicToplevelManagerV1,
    },
    workspace::v1::client::{
        zcosmic_workspace_group_handle_v1::{self, ZcosmicWorkspaceGroupHandleV1},
        zcosmic_workspace_handle_v1::{self, ZcosmicWorkspaceHandleV1},
        zcosmic_workspace_manager_v1::{self, ZcosmicWorkspaceManagerV1},
    },
};
use switcher_config::WorkspaceScope;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToplevelEntry {
    pub app_id:     String,
    pub title:      String,
    pub is_active:  bool,
    pub handle_key: u64,
}

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub generation: String,
    pub toplevels: Vec<ToplevelEntry>,
}

#[derive(Serialize, Deserialize)]
enum Request {
    List(WorkspaceScope),
    Activate { generation: String, key: u64 },
    Ping,
}

#[derive(Serialize, Deserialize)]
enum Response {
    List(Snapshot),
    Ok,
    Error(String),
}

pub fn daemon_socket_path() -> Result<std::path::PathBuf> {
    Ok(crate::runtime_dir()?.join("cosmic-ext-app-switcher-mru.sock"))
}

fn request(request: &Request) -> Result<Response> {
    let mut stream = UnixStream::connect(daemon_socket_path()?)
        .context("MRU service is not running. Run: systemctl --user restart cosmic-ext-app-switcher-mru.service")?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    serde_json::to_writer(&mut stream, request)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut bytes = Vec::new();
    stream.take(4 * 1024 * 1024).read_to_end(&mut bytes)?;
    let reply: Response = serde_json::from_slice(&bytes)?;
    if let Response::Error(message) = &reply { return Err(anyhow!("{message}")); }
    Ok(reply)
}

pub fn snapshot(scope: WorkspaceScope) -> Result<Snapshot> {
    check_scope(scope)?;
    match request(&Request::List(scope))? {
        Response::List(snapshot) => Ok(snapshot),
        _ => Err(anyhow!("Unexpected MRU service response")),
    }
}

fn check_scope(scope: WorkspaceScope) -> Result<()> {
    if scope == WorkspaceScope::CurrentWorkspace {
        return Err(anyhow!("Current workspace is unsupported by the pinned legacy Wayland protocol. Choose All workspaces or Current monitor in the switcher settings."));
    }
    Ok(())
}

pub fn activate(generation: &str, key: u64) -> Result<()> {
    match request(&Request::Activate { generation: generation.into(), key })? {
        Response::Ok => Ok(()),
        _ => Err(anyhow!("Unexpected MRU activation response")),
    }
}

pub fn ping() -> Result<()> {
    match request(&Request::Ping)? {
        Response::Ok => Ok(()),
        _ => Err(anyhow!("Unexpected MRU health response")),
    }
}

// Versions we bind at. The compositor has to keep speaking the version a client bound, so
// newer versions it adds later don't affect us. zcosmic_toplevel_info_v1 must stay at 1:
// v2+ never emits Toplevel events.
const TOPLEVEL_INFO_VERSION:    u32 = 1;
const TOPLEVEL_MANAGER_VERSION: u32 = 1;
const SEAT_VERSION:             u32 = 7;

/// Globals the switcher can't work without, and the lowest version of each it accepts.
/// zwlr_layer_shell_v1 is bound by libcosmic on its own connection (for the overlay), not
/// by us, but cosmic-comp gates it the same way as the toplevel protocols, so it's checked
/// here too. libcosmic's toolkit accepts any version from 1.
const REQUIRED_GLOBALS: &[(&str, u32)] = &[
    ("zcosmic_toplevel_info_v1",    TOPLEVEL_INFO_VERSION),
    ("zcosmic_toplevel_manager_v1", TOPLEVEL_MANAGER_VERSION),
    ("zwlr_layer_shell_v1",         1),
    ("wl_seat",                     SEAT_VERSION),
];

pub struct GlobalStatus {
    interface:  &'static str,
    needed:     u32,
    advertised: Option<u32>,  // highest version offered, None if not offered at all
}

impl GlobalStatus {
    pub fn ok(&self) -> bool {
        self.advertised.is_some_and(|v| v >= self.needed)
    }
}

impl std::fmt::Display for GlobalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (interface, needed) = (self.interface, self.needed);
        match self.advertised {
            Some(v) if v >= needed => write!(f, "{interface}: ok (v{v}, needs v{needed})"),
            Some(v) => write!(f, "{interface}: TOO OLD (v{v}, needs v{needed})"),
            None    => write!(f, "{interface}: NOT offered (needs v{needed})"),
        }
    }
}

fn required_globals_status(advertised: &[(String, u32)]) -> Vec<GlobalStatus> {
    REQUIRED_GLOBALS.iter()
        .map(|&(interface, needed)| GlobalStatus {
            interface,
            needed,
            advertised: advertised.iter()
                .filter(|(i, _)| i == interface)
                .map(|&(_, v)| v)
                .max(),
        })
        .collect()
}

/// Connects to the compositor and reports on every required global, for `--check-compat`.
pub fn probe_required_globals() -> Result<Vec<GlobalStatus>> {
    let conn = Connection::connect_to_env()
        .context("could not connect to the Wayland compositor (is COSMIC running?)")?;
    let (globals, _queue) = registry_queue_init::<RegistryProbe>(&conn)?;
    let advertised: Vec<(String, u32)> = globals.contents().clone_list()
        .into_iter()
        .map(|g| (g.interface, g.version))
        .collect();
    Ok(required_globals_status(&advertised))
}

struct AppData {
    history: History,
    next_key: u64,
    generation: String,
    toplevels:          Vec<Toplevel>,
    _info:              Option<ZcosmicToplevelInfoV1>,  // must stay alive to receive events
    manager:            Option<ZcosmicToplevelManagerV1>,
    seat:               Option<wl_seat::WlSeat>,
    _workspace_manager: Option<ZcosmicWorkspaceManagerV1>,
    outputs:            Vec<wl_output::WlOutput>,
    // cosmic-comp does a one-time sync of each toplevel's output membership at handle
    // creation time, using whatever wl_output binds the client already has — so we must
    // not bind zcosmic_toplevel_info_v1 (which creates toplevel handles) until our own
    // wl_output binds have been sent first. Deferred here and bound explicitly after the
    // registry listing is fully drained, rather than reactively as its Global arrives.
    toplevel_info_name: Option<u32>,
    globals:            Vec<(String, u32)>,  // every (interface, version) advertised
}

struct Toplevel {
    key: u64,
    handle:     ZcosmicToplevelHandleV1,
    app_id:     String,
    title:      String,
    is_active:  bool,
    outputs:    Vec<wl_output::WlOutput>,
    workspaces: Vec<ZcosmicWorkspaceHandleV1>,
}

pub fn run_daemon() -> Result<()> {
    // Lock before removing a stale socket, so a second daemon cannot steal it.
    let _lock = crate::file_lock("cosmic-ext-app-switcher-mru.lock", true)?;
    let conn    = Connection::connect_to_env()?;
    let display = conn.display();
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();

    let registry = display.get_registry(&qh, ());

    let mut data = AppData {
        history: History::default(),
        next_key: 0,
        generation: format!("{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()),
        toplevels: vec![],
        _info: None,
        manager: None,
        seat: None,
        _workspace_manager: None,
        outputs: vec![],
        toplevel_info_name: None,
        globals: vec![],
    };

    // First roundtrip: drain the full registry listing. wl_output/wl_seat/workspace-manager
    // are bound immediately as their Globals arrive; zcosmic_toplevel_info_v1's bind is
    // deferred (see AppData::toplevel_info_name) regardless of where in the listing it
    // appears.
    queue.roundtrip(&mut data)?;

    // Without this, a missing global means an empty window list and a silent exit (or an
    // overlay whose selection can't be activated), which looks like a bug in the switcher.
    let missing: Vec<String> = required_globals_status(&data.globals).iter()
        .filter(|s| !s.ok())
        .map(ToString::to_string)
        .collect();
    if !missing.is_empty() {
        return Err(anyhow!(
            "cosmic-ext-app-switcher can't run: the compositor doesn't offer the Wayland \
             interfaces it needs ({}). A COSMIC update may have removed or changed them, or \
             the switcher is running inside a sandbox such as Flatpak. Run \
             `cosmic-ext-app-switcher --check-compat` for details.",
            missing.join("; "),
        ));
    }

    // Second roundtrip: force the server to fully process our wl_output bind requests
    // before we bind toplevel_info below — otherwise toplevel handles it creates may miss
    // their initial output_enter sync (see comment on toplevel_info_name).
    queue.roundtrip(&mut data)?;

    if let Some(name) = data.toplevel_info_name.take() {
        data._info = Some(
            registry.bind::<ZcosmicToplevelInfoV1, _, _>(name, TOPLEVEL_INFO_VERSION, &qh, ())
        );
    }

    // Extra roundtrips over the original three: workspace-group/workspace handles and
    // toplevel handles both need time to fully cascade before we read state back out.
    for _ in 0..5 {
        queue.roundtrip(&mut data)?;
    }

    let path = daemon_socket_path()?;
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let _cleanup = crate::ipc::OwnedSocket::new(path)?;
    log::info!("MRU service ready; history starts now");

    loop {
        queue.dispatch_pending(&mut data)?;
        conn.flush()?;
        let Some(guard) = queue.prepare_read() else { continue; };
        let mut fds = [
            libc::pollfd { fd: conn.backend().poll_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: listener.as_raw_fd(), events: libc::POLLIN, revents: 0 },
        ];
        // No periodic polling: sleep until the compositor or a client has an event.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if ready < 0 {
            drop(guard);
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted { continue; }
            return Err(error.into());
        }
        if fds.iter().any(|fd| fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0) {
            return Err(anyhow!("Wayland connection or MRU socket closed/failed"));
        }
        if fds[0].revents != 0 {
            guard.read()?;
            queue.dispatch_pending(&mut data)?;
        } else { drop(guard); }
        if fds[1].revents & libc::POLLIN != 0 {
            let (mut stream, _) = match listener.accept() {
                Ok(client) => client,
                Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => continue,
                Err(error) => return Err(error.into()),
            };
            // Clients are same-user only. Bound reads so an interrupted client cannot hang us.
            stream.set_read_timeout(Some(Duration::from_millis(500)))?;
            stream.set_write_timeout(Some(Duration::from_millis(500)))?;
            let mut bytes = Vec::new();
            if (&mut stream).take(65536).read_to_end(&mut bytes).is_err() { continue; }
            let reply = match serde_json::from_slice::<Request>(&bytes) {
                Ok(Request::List(scope)) => {
                    if let Err(error) = check_scope(scope) {
                        Response::Error(error.to_string())
                    } else {
                        // Drain focus events before freezing the UI snapshot.
                        queue.roundtrip(&mut data)?;
                        Response::List(data.snapshot(scope))
                    }
                }
                Ok(Request::Activate { generation, key }) => {
                    queue.roundtrip(&mut data)?;
                    if generation != data.generation {
                        Response::Error("MRU service restarted; reopen Alt+Tab".into())
                    } else if let Some(t) = data.toplevels.iter().find(|t| t.key == key) {
                        if let (Some(manager), Some(seat)) = (&data.manager, &data.seat) {
                            manager.activate(&t.handle, seat);
                            // Flush and acknowledge server processing before the UI can exit.
                            // History changes ONLY on compositor-confirmed Activated events.
                            queue.roundtrip(&mut data)?;
                            Response::Ok
                        } else { Response::Error("Activation protocol unavailable".into()) }
                    } else { Response::Error("Selected window has already closed".into()) }
                }
                Ok(Request::Ping) => Response::Ok,
                Err(error) => Response::Error(error.to_string()),
            };
            // A closed UI must not terminate the background service.
            let _ = serde_json::to_writer(&mut stream, &reply);
            let _ = stream.flush();
        }
    }
}

impl AppData {
    fn snapshot(&self, scope: WorkspaceScope) -> Snapshot {
        let reference = self.toplevels.iter()
            .filter(|t| !t.app_id.is_empty() || !t.title.is_empty())
            .min_by_key(|t| self.history.sort_key(t.key));
        let active_outputs = reference.map(|t| t.outputs.as_slice()).unwrap_or(&[]);
        let mut entries: Vec<_> = self.toplevels.iter()
            .filter(|t| !t.app_id.is_empty() || !t.title.is_empty())
            .filter(|t| match scope {
                WorkspaceScope::AllWorkspaces => true,
                // Rejected by check_scope before reaching this filter.
                WorkspaceScope::CurrentWorkspace => false,
                WorkspaceScope::CurrentOutput => active_outputs.is_empty()
                    || t.outputs.iter().any(|o| active_outputs.contains(o)),
            })
            .map(|t| ToplevelEntry {
                app_id: t.app_id.clone(), title: t.title.clone(),
                is_active: t.is_active, handle_key: t.key,
            }).collect();
        entries.sort_by_key(|t| self.history.sort_key(t.handle_key));
        Snapshot { generation: self.generation.clone(), toplevels: entries }
    }
}

// --- Registry: bind our three globals ---

impl Dispatch<wl_registry::WlRegistry, ()> for AppData {
    fn event(
        data: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            data.globals.push((interface.clone(), version));
            // A global offered below the version we bind is left unbound: binding it would
            // be a protocol error that kills the connection before the required-globals
            // check in wayland_thread_main can report it.
            match interface.as_str() {
                "zcosmic_toplevel_info_v1" if version >= TOPLEVEL_INFO_VERSION => {
                    // Binding deferred until after wl_output is bound — see
                    // AppData::toplevel_info_name and wayland_thread_main.
                    data.toplevel_info_name = Some(name);
                }
                "zcosmic_toplevel_manager_v1" if version >= TOPLEVEL_MANAGER_VERSION => {
                    data.manager = Some(registry.bind::<ZcosmicToplevelManagerV1, _, _>(
                        name, TOPLEVEL_MANAGER_VERSION, qh, (),
                    ));
                }
                "wl_seat" if version >= SEAT_VERSION => {
                    if data.seat.is_none() {
                        data.seat = Some(
                            registry.bind::<wl_seat::WlSeat, _, _>(name, SEAT_VERSION, qh, ())
                        );
                    }
                }
                "wl_output" => {
                    data.outputs.push(
                        registry.bind::<wl_output::WlOutput, _, _>(name, 1, qh, ())
                    );
                }
                "zcosmic_workspace_manager_v1" => {
                    data._workspace_manager = Some(
                        registry.bind::<ZcosmicWorkspaceManagerV1, _, _>(name, 1, qh, ())
                    );
                }
                _ => {}
            }
        }
    }
}

// --- ZcosmicToplevelInfoV1: new toplevel events ---

impl Dispatch<ZcosmicToplevelInfoV1, ()> for AppData {
    fn event(
        data: &mut Self,
        _: &ZcosmicToplevelInfoV1,
        event: zcosmic_toplevel_info_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zcosmic_toplevel_info_v1::Event::Toplevel { toplevel } = event {
            data.next_key += 1;
            data.history.open(data.next_key);
            data.toplevels.push(Toplevel {
                key: data.next_key,
                handle:     toplevel,
                app_id:     String::new(),
                title:      String::new(),
                is_active:  false,
                outputs:    vec![],
                workspaces: vec![],
            });
        }
    }

    event_created_child!(AppData, ZcosmicToplevelInfoV1, [
        zcosmic_toplevel_info_v1::EVT_TOPLEVEL_OPCODE =>
            (ZcosmicToplevelHandleV1, ()),
    ]);
}

// --- ZcosmicToplevelHandleV1: per-window metadata ---

impl Dispatch<ZcosmicToplevelHandleV1, ()> for AppData {
    fn event(
        data: &mut Self,
        handle: &ZcosmicToplevelHandleV1,
        event: zcosmic_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(t) = data.toplevels.iter_mut().find(|t| &t.handle == handle) else {
            return;
        };
        match event {
            zcosmic_toplevel_handle_v1::Event::AppId { app_id } => {
                t.app_id = app_id;
            }
            zcosmic_toplevel_handle_v1::Event::Title { title } => {
                t.title = title;
            }
            zcosmic_toplevel_handle_v1::Event::State { state } => {
                let activated = zcosmic_toplevel_handle_v1::State::Activated as u32;
                t.is_active = state
                    .chunks_exact(4)
                    .any(|b| u32::from_ne_bytes(b.try_into().unwrap()) == activated);
                data.history.state(t.key, t.is_active);
            }
            zcosmic_toplevel_handle_v1::Event::Closed => {
                data.history.close(t.key);
                data.toplevels.retain(|t| &t.handle != handle);
            }
            zcosmic_toplevel_handle_v1::Event::OutputEnter { output } => {
                if !t.outputs.contains(&output) {
                    t.outputs.push(output);
                }
            }
            zcosmic_toplevel_handle_v1::Event::OutputLeave { output } => {
                t.outputs.retain(|o| o != &output);
            }
            zcosmic_toplevel_handle_v1::Event::WorkspaceEnter { workspace } => {
                if !t.workspaces.contains(&workspace) {
                    t.workspaces.push(workspace);
                }
            }
            zcosmic_toplevel_handle_v1::Event::WorkspaceLeave { workspace } => {
                t.workspaces.retain(|w| w != &workspace);
            }
            _ => {}
        }
    }
}

// --- Registry for probe_required_globals: registry_queue_init collects the global list
// itself, so there's nothing to handle here ---

struct RegistryProbe;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for RegistryProbe {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event,
             _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

// --- No-op dispatches for manager and seat ---

impl Dispatch<ZcosmicToplevelManagerV1, ()> for AppData {
    fn event(_: &mut Self, _: &ZcosmicToplevelManagerV1,
             _: zcosmic_toplevel_manager_v1::Event, _: &(),
             _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_seat::WlSeat, ()> for AppData {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event,
             _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_output::WlOutput, ()> for AppData {
    fn event(_: &mut Self, _: &wl_output::WlOutput, _: wl_output::Event,
             _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

// --- Workspace hierarchy: bound only so zcosmic_toplevel_handle_v1's workspace_enter/
// leave events have valid zcosmic_workspace_handle_v1 objects to reference. We don't
// need names/coordinates/active-state off these — membership identity is enough to
// compare against the currently-active toplevel's own workspace list.

impl Dispatch<ZcosmicWorkspaceManagerV1, ()> for AppData {
    fn event(_: &mut Self, _: &ZcosmicWorkspaceManagerV1,
             _: zcosmic_workspace_manager_v1::Event, _: &(),
             _: &Connection, _: &QueueHandle<Self>) {}

    event_created_child!(AppData, ZcosmicWorkspaceManagerV1, [
        zcosmic_workspace_manager_v1::EVT_WORKSPACE_GROUP_OPCODE =>
            (ZcosmicWorkspaceGroupHandleV1, ()),
    ]);
}

impl Dispatch<ZcosmicWorkspaceGroupHandleV1, ()> for AppData {
    fn event(_: &mut Self, _: &ZcosmicWorkspaceGroupHandleV1,
             _: zcosmic_workspace_group_handle_v1::Event, _: &(),
             _: &Connection, _: &QueueHandle<Self>) {}

    event_created_child!(AppData, ZcosmicWorkspaceGroupHandleV1, [
        zcosmic_workspace_group_handle_v1::EVT_WORKSPACE_OPCODE =>
            (ZcosmicWorkspaceHandleV1, ()),
    ]);
}

impl Dispatch<ZcosmicWorkspaceHandleV1, ()> for AppData {
    fn event(_: &mut Self, _: &ZcosmicWorkspaceHandleV1,
             _: zcosmic_workspace_handle_v1::Event, _: &(),
             _: &Connection, _: &QueueHandle<Self>) {}
}

#[cfg(test)]
mod tests {
    use super::required_globals_status;

    #[test]
    fn unsupported_workspace_scope_cannot_silently_return_all_windows() {
        use switcher_config::WorkspaceScope;
        assert!(super::check_scope(WorkspaceScope::CurrentWorkspace).is_err());
        assert!(super::check_scope(WorkspaceScope::AllWorkspaces).is_ok());
        assert!(super::check_scope(WorkspaceScope::CurrentOutput).is_ok());
    }

    fn advertised(globals: &[(&str, u32)]) -> Vec<(String, u32)> {
        globals.iter().map(|&(i, v)| (i.to_string(), v)).collect()
    }

    fn line_for(globals: &[(&str, u32)], interface: &str) -> String {
        required_globals_status(&advertised(globals)).iter()
            .map(ToString::to_string)
            .find(|l| l.starts_with(&format!("{interface}:")))
            .unwrap()
    }

    #[test]
    fn everything_offered_is_ok() {
        let globals = [
            ("zcosmic_toplevel_info_v1", 3),
            ("zcosmic_toplevel_manager_v1", 4),
            ("zwlr_layer_shell_v1", 5),
            ("wl_seat", 9),
        ];
        assert!(required_globals_status(&advertised(&globals)).iter().all(|s| s.ok()));
    }

    #[test]
    fn missing_global_is_reported() {
        let globals = [("zcosmic_toplevel_info_v1", 3), ("zwlr_layer_shell_v1", 5), ("wl_seat", 9)];
        let statuses = required_globals_status(&advertised(&globals));
        assert_eq!(statuses.iter().filter(|s| !s.ok()).count(), 1);
        assert_eq!(
            line_for(&globals, "zcosmic_toplevel_manager_v1"),
            "zcosmic_toplevel_manager_v1: NOT offered (needs v1)",
        );
    }

    #[test]
    fn version_below_the_one_we_bind_is_too_old() {
        let globals = [("wl_seat", 5)];
        assert_eq!(line_for(&globals, "wl_seat"), "wl_seat: TOO OLD (v5, needs v7)");
    }

    /// A seat advertised twice counts as its highest version.
    #[test]
    fn repeated_global_uses_highest_version() {
        let globals = [("wl_seat", 5), ("wl_seat", 8)];
        assert_eq!(line_for(&globals, "wl_seat"), "wl_seat: ok (v8, needs v7)");
    }
}
