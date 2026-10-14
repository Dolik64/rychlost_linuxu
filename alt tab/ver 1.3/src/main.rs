use anyhow::Result;
use clap::Parser;
use cosmic::cosmic_config::ConfigGet;
use std::io::Write;
use std::os::{
    fd::AsRawFd,
    unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
};
use std::sync::OnceLock;
use switcher_config::{Theme, WorkspaceScope, APP_ID, CONFIG_VERSION};

pub static UI_LISTENER: OnceLock<UnixListener> = OnceLock::new();

pub fn runtime_dir() -> Result<std::path::PathBuf> {
    let path = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            anyhow::anyhow!("Run inside your logged-in COSMIC desktop (XDG_RUNTIME_DIR is missing)")
        })?;
    Ok(path)
}

pub fn file_lock(name: &str, nonblocking: bool) -> Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(runtime_dir()?.join(name))?;
    let operation = libc::LOCK_EX | if nonblocking { libc::LOCK_NB } else { 0 };
    if unsafe { libc::flock(file.as_raw_fd(), operation) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(file)
}

pub struct SocketCleanup(pub std::path::PathBuf);
impl Drop for SocketCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn socket_path() -> std::path::PathBuf {
    runtime_dir()
        .expect("runtime checked in main")
        .join("cosmic-ext-app-switcher.sock")
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
    if args.daemon {
        return wayland::run_daemon();
    }
    if args.mru_check {
        wayland::ping()?;
        println!("MRU service OK");
        return Ok(());
    }
    if args.mru_list {
        println!(
            "{}",
            serde_json::to_string_pretty(&wayland::snapshot(load_scope())?)?
        );
        return Ok(());
    }
    if args.check_compat {
        return check_compat();
    }

    // Serialize startup so rapid shortcut invocations cannot open two overlays.
    let startup_lock = file_lock("cosmic-ext-app-switcher-ui.lock", false)?;
    let cmd = if args.reverse {
        b"prev" as &[u8]
    } else {
        b"next" as &[u8]
    };
    if let Ok(mut stream) = UnixStream::connect(socket_path()) {
        stream.set_write_timeout(Some(std::time::Duration::from_secs(1)))?;
        stream.write_all(cmd)?;
        return Ok(());
    }
    let _ = std::fs::remove_file(socket_path());
    let listener = UnixListener::bind(socket_path())?;
    std::fs::set_permissions(socket_path(), std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    UI_LISTENER
        .set(listener)
        .map_err(|_| anyhow::anyhow!("UI socket already initialized"))?;
    let _cleanup = SocketCleanup(socket_path());

    let snapshot = wayland::snapshot(load_scope())?;
    drop(startup_lock);
    if snapshot.toplevels.is_empty() {
        return Ok(());
    }
    let res = app::run(
        snapshot.toplevels,
        args.reverse,
        snapshot.generation,
        load_theme(),
    );
    let _ = std::fs::remove_file(socket_path());
    res
}

// ============================================================================
// MODULE: mru
// ============================================================================
mod mru {
    use std::{cmp::Reverse, collections::HashMap};

    #[derive(Default)]
    pub struct History {
        clock: u64,
        windows: HashMap<u64, (bool, u64)>,
    }

    impl History {
        pub fn open(&mut self, id: u64) {
            self.windows.entry(id).or_insert((false, 0));
        }
        pub fn close(&mut self, id: u64) {
            self.windows.remove(&id);
        }
        pub fn state(&mut self, id: u64, activated: bool) {
            let Some((was_active, last_used)) = self.windows.get_mut(&id) else {
                return;
            };
            if activated && !*was_active {
                self.clock += 1;
                *last_used = self.clock;
            }
            *was_active = activated;
        }
        pub fn sort_key(&self, id: u64) -> (Reverse<u64>, u64) {
            (
                Reverse(self.windows.get(&id).map(|w| w.1).unwrap_or(0)),
                id,
            )
        }
    }
}

// ============================================================================
// MODULE: icons
// ============================================================================
mod icons {
    use cosmic::widget::icon;
    use freedesktop_desktop_entry::{DesktopEntry, Iter as DesktopIter};
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    const FALLBACK_ICON: &str = "application-x-executable";

    pub enum AppIcon {
        Named(String),
        File(PathBuf),
    }

    pub struct AppVisual {
        pub icon: AppIcon,
        pub label: String,
    }

    pub fn visual_for(app_id: &str, window_title: &str, size: u16) -> AppVisual {
        if app_id.starts_with("steam_app_") {
            return AppVisual {
                icon: resolve("steam", size),
                label: first_non_empty(&[window_title, "Steam"]).to_string(),
            };
        }

        let entry = desktop_entry_for(app_id);
        let label = entry
            .as_ref()
            .and_then(|e| e.name.as_deref())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| first_non_empty(&[window_title, app_id, "Unknown"]))
            .to_string();

        let icon = match entry.and_then(|e| e.icon) {
            Some(field) if Path::new(&field).is_absolute() && Path::new(&field).is_file() => {
                AppIcon::File(PathBuf::from(field))
            }
            Some(field) => resolve(&icon_name_from_field(&field), size),
            None => resolve(app_id.split('.').next_back().unwrap_or(app_id), size),
        };

        AppVisual { icon, label }
    }

    fn first_non_empty<'a>(candidates: &[&'a str]) -> &'a str {
        candidates
            .iter()
            .copied()
            .find(|c| !c.is_empty())
            .unwrap_or_default()
    }

    fn resolve(name: &str, size: u16) -> AppIcon {
        if icon::from_name(name).size(size).path().is_some() {
            return AppIcon::Named(name.to_string());
        }
        AppIcon::Named(FALLBACK_ICON.to_string())
    }

    struct DesktopInfo {
        icon: Option<String>,
        name: Option<String>,
    }

    fn desktop_entry_for(app_id: &str) -> Option<DesktopInfo> {
        if app_id.is_empty() {
            return None;
        }
        let dirs = search_dirs();

        for dir in &dirs {
            if let Some(info) = read_entry(&dir.join(format!("{app_id}.desktop"))) {
                return Some(info);
            }
        }

        for path in DesktopIter::new(dirs.clone()) {
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Ok(s) = std::str::from_utf8(&bytes) else { continue };
            let Ok(entry) = DesktopEntry::decode(&path, s) else { continue };
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let claims = entry
                .startup_wm_class()
                .is_some_and(|c| c.eq_ignore_ascii_case(app_id))
                || stem_matches(stem, app_id);
            if claims {
                return Some(DesktopInfo {
                    icon: entry.icon().map(ToString::to_string),
                    name: entry.name(None).map(|n| n.to_string()),
                });
            }
        }
        None
    }

    fn stem_matches(stem: &str, app_id: &str) -> bool {
        stem.eq_ignore_ascii_case(app_id)
            || stem
                .rsplit('.')
                .next()
                .is_some_and(|last| last.eq_ignore_ascii_case(app_id))
    }

    fn read_entry(path: &Path) -> Option<DesktopInfo> {
        let bytes = std::fs::read(path).ok()?;
        let s = std::str::from_utf8(&bytes).ok()?;
        let entry = DesktopEntry::decode(path, s).ok()?;
        Some(DesktopInfo {
            icon: entry.icon().map(ToString::to_string),
            name: entry.name(None).map(|n| n.to_string()),
        })
    }

    fn search_dirs() -> Vec<PathBuf> {
        let home = std::env::var("HOME").unwrap_or_default();
        let data_home = std::env::var("XDG_DATA_HOME")
            .ok()
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| format!("{home}/.local/share"));
        let data_dirs = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());

        let xdg = std::iter::once(data_home.clone())
            .chain(data_dirs.split(':').map(ToString::to_string))
            .filter(|d| !d.is_empty())
            .map(|d| PathBuf::from(d).join("applications"));

        let extra = [
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
            PathBuf::from(format!("{data_home}/flatpak/exports/share/applications")),
            PathBuf::from("/var/lib/snapd/desktop/applications"),
        ];

        let mut seen = HashSet::new();
        xdg.chain(extra).filter(|d| seen.insert(d.clone())).collect()
    }

    fn icon_name_from_field(icon: &str) -> String {
        if icon.starts_with('/') {
            Path::new(icon)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(icon)
                .to_string()
        } else {
            icon.to_string()
        }
    }
}

// ============================================================================
// MODULE: app
// ============================================================================
mod app {
    use anyhow::Result;
    use cosmic::{
        app::{Application, Core, Settings, Task},
        iced::{
            self, event,
            keyboard::{self, key::Named},
            platform_specific::{
                runtime::wayland::layer_surface::SctkLayerSurfaceSettings,
                shell::wayland::commands::layer_surface::{
                    destroy_layer_surface, get_layer_surface, Anchor, KeyboardInteractivity, Layer,
                },
            },
            window::Id as WindowId,
            Event, Subscription,
        },
        Element,
    };
    use std::time::Duration;
    use switcher_config::{Theme, ThemeValues};

    use crate::icons::{visual_for, AppVisual};
    use crate::wayland::ToplevelEntry;

    pub struct AppSwitcher {
        core: Core,
        pub toplevels: Vec<ToplevelEntry>,
        pub visuals: Vec<AppVisual>,
        pub selected: usize,
        generation: String,
        pub finishing: bool,
        pub ready: bool,
        super_held: bool,
        alt_held: bool,
        surface_id: WindowId,
        pub theme: ThemeValues,
    }

    #[derive(Debug, Clone)]
    pub enum Message {
        SelectNext,
        SelectPrev,
        SelectIndex(usize),
        Activate,
        Cancel,
        Ready,
        KeyEvent(keyboard::Event),
    }

    impl Application for AppSwitcher {
        type Executor = cosmic::executor::Default;
        type Flags = (Vec<ToplevelEntry>, bool, String, Theme);
        type Message = Message;

        const APP_ID: &'static str = "io.github.cosmic-ext-app-switcher";

        fn core(&self) -> &Core {
            &self.core
        }
        fn core_mut(&mut self) -> &mut Core {
            &mut self.core
        }

        fn init(core: Core, flags: Self::Flags) -> (Self, Task<Message>) {
            let (toplevels, reverse, generation, theme_preset) = flags;
            let n = toplevels.len();
            let selected = if reverse {
                n.saturating_sub(1)
            } else {
                1.min(n.saturating_sub(1))
            };

            let theme = theme_preset.values();
            let visuals: Vec<AppVisual> = toplevels
                .iter()
                .map(|entry| visual_for(&entry.app_id, &entry.title, theme.icon_size))
                .collect();

            let (surface_w, surface_h) = crate::ui::surface_size(toplevels.len(), &theme);

            let surface_id = WindowId::unique();
            let layer_task = get_layer_surface::<cosmic::Action<Message>>(SctkLayerSurfaceSettings {
                id: surface_id,
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                anchor: Anchor::empty(),
                size: Some((Some(surface_w), Some(surface_h))),
                ..Default::default()
            });

            let ready_task = Task::perform(
                async {
                    tokio::time::sleep(Duration::from_millis(28)).await;
                },
                |_| cosmic::Action::App(Message::Ready),
            );

            (
                AppSwitcher {
                    core,
                    toplevels,
                    visuals,
                    selected,
                    generation,
                    finishing: false,
                    ready: false,
                    super_held: false,
                    alt_held: true,
                    surface_id,
                    theme,
                },
                Task::batch([layer_task, ready_task]),
            )
        }

        fn update(&mut self, msg: Message) -> Task<Message> {
            if self.finishing {
                return Task::none();
            }
            let n = self.toplevels.len();
            if n == 0 {
                return iced::exit();
            }
            match msg {
                Message::Ready => {
                    self.ready = true;
                }
                Message::SelectNext => {
                    self.selected = (self.selected + 1) % n;
                }
                Message::SelectPrev => {
                    self.selected = (self.selected + n - 1) % n;
                }
                Message::SelectIndex(i) => {
                    if i < n {
                        self.selected = i;
                    }
                }
                Message::Activate => {
                    self.finishing = true;
                    self.ready = false;
                    let _ = std::fs::remove_file(crate::socket_path());
                    if let Err(error) = crate::wayland::activate(
                        &self.generation,
                        self.toplevels[self.selected].handle_key,
                    ) {
                        log::error!("Could not activate selected window: {error:#}");
                    }
                    return Task::batch([destroy_layer_surface(self.surface_id), iced::exit()]);
                }
                Message::Cancel => {
                    self.finishing = true;
                    self.ready = false;
                    let _ = std::fs::remove_file(crate::socket_path());
                    return Task::batch([destroy_layer_surface(self.surface_id), iced::exit()]);
                }
                Message::KeyEvent(ke) => match ke {
                    keyboard::Event::KeyPressed { key, modifiers, .. } => match key {
                        iced::keyboard::Key::Named(Named::Tab) => {
                            return self.update(if modifiers.shift() {
                                Message::SelectPrev
                            } else {
                                Message::SelectNext
                            });
                        }
                        iced::keyboard::Key::Named(Named::ArrowRight)
                        | iced::keyboard::Key::Named(Named::ArrowDown) => {
                            return self.update(Message::SelectNext);
                        }
                        iced::keyboard::Key::Named(Named::ArrowLeft)
                        | iced::keyboard::Key::Named(Named::ArrowUp) => {
                            return self.update(Message::SelectPrev);
                        }
                        iced::keyboard::Key::Named(Named::Escape) => {
                            return self.update(Message::Cancel);
                        }
                        iced::keyboard::Key::Named(Named::Enter) => {
                            return self.update(Message::Activate);
                        }
                        _ => {}
                    },
                    keyboard::Event::KeyReleased { key, .. } => match key {
                        iced::keyboard::Key::Named(Named::Alt)
                        | iced::keyboard::Key::Named(Named::AltGraph)
                        | iced::keyboard::Key::Named(Named::Super) => {
                            return self.update(Message::Activate);
                        }
                        _ => {}
                    },
                    keyboard::Event::ModifiersChanged(mods) => {
                        if !mods.alt() && !mods.logo() {
                            return self.update(Message::Activate);
                        }
                        self.super_held = mods.logo();
                        self.alt_held = mods.alt();
                    }
                },
            }
            Task::none()
        }

        fn subscription(&self) -> Subscription<Message> {
            let key_sub = event::listen_with(|event, _status, _window| match event {
                Event::Keyboard(ke) => Some(Message::KeyEvent(ke)),
                Event::Window(iced::window::Event::Unfocused) => Some(Message::Cancel),
                _ => None,
            });

            let socket_sub = {
                use cosmic::iced::futures::SinkExt;
                use std::any::TypeId;
                struct SocketSub;
                Subscription::run_with(TypeId::of::<SocketSub>(), |_| {
                    cosmic::iced::stream::channel(
                        16,
                        |mut tx: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                            use tokio::io::AsyncReadExt;
                            let listener = tokio::net::UnixListener::from_std(
                                crate::UI_LISTENER
                                    .get()
                                    .expect("UI socket initialized")
                                    .try_clone()
                                    .expect("clone UI socket"),
                            )
                            .expect("nonblocking UI socket");
                            loop {
                                if let Ok((mut stream, _)) = listener.accept().await {
                                    let mut buf = Vec::new();
                                    let read = tokio::time::timeout(
                                        std::time::Duration::from_secs(1),
                                        (&mut stream).take(16).read_to_end(&mut buf),
                                    )
                                    .await;
                                    if !matches!(read, Ok(Ok(_))) {
                                        continue;
                                    }
                                    let msg = match buf.as_slice() {
                                        b"next" => Message::SelectNext,
                                        b"prev" => Message::SelectPrev,
                                        _ => continue,
                                    };
                                    let _ = tx.send(msg).await;
                                }
                            }
                        },
                    )
                })
            };

            Subscription::batch([key_sub, socket_sub])
        }

        fn view(&self) -> Element<'_, Message> {
            crate::ui::view(self)
        }

        fn view_window(&self, _id: WindowId) -> Element<'_, Message> {
            crate::ui::view(self)
        }
    }

    pub fn run(
        toplevels: Vec<ToplevelEntry>,
        reverse: bool,
        generation: String,
        theme: Theme,
    ) -> Result<()> {
        let settings = Settings::default().no_main_window(true).transparent(true);
        cosmic::app::run::<AppSwitcher>(settings, (toplevels, reverse, generation, theme))
            .map_err(|e| anyhow::anyhow!("{e:?}"))
    }
}

// ============================================================================
// MODULE: ui
// ============================================================================
mod ui {
    use cosmic::{
        iced::{
            widget::container::Style as ContainerStyle, Alignment, Background, Border, Color,
            Length, Shadow, Vector,
        },
        widget::{column, container, icon, mouse_area, row, text, Space},
        Element,
    };
    use switcher_config::ThemeValues;

    use crate::app::{AppSwitcher, Message};
    use crate::icons::AppIcon;

    const CELL_PAD: u16 = 10;
    const CELL_SPACING: u16 = 4;
    const STRIP_PAD_X: u16 = 18;
    const STRIP_PAD_Y: u16 = 14;
    const TITLE_SIZE: u16 = 14;
    const TITLE_HEIGHT: u16 = 20;
    const TITLE_GAP: u16 = 6;
    const SURFACE_MARGIN: u32 = 40;

    pub fn surface_size(window_count: usize, theme: &ThemeValues) -> (u32, u32) {
        let n = window_count as u32;
        let cell = u32::from(theme.icon_size) + 2 * u32::from(CELL_PAD);
        let strip_w =
            n * cell + n.saturating_sub(1) * u32::from(CELL_SPACING) + 2 * u32::from(STRIP_PAD_X);
        let strip_h = cell
            + u32::from(TITLE_GAP)
            + u32::from(TITLE_HEIGHT)
            + 2 * u32::from(STRIP_PAD_Y);
        (strip_w + SURFACE_MARGIN, strip_h + SURFACE_MARGIN)
    }

    fn fit_title(title: &str, surface_w: u32) -> String {
        let usable = surface_w
            .saturating_sub(SURFACE_MARGIN)
            .saturating_sub(2 * u32::from(STRIP_PAD_X));
        let max_chars = (usable / (u32::from(TITLE_SIZE) * 55 / 100)).max(3) as usize;
        if title.chars().count() <= max_chars {
            return title.to_string();
        }
        let kept: String = title.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", kept.trim_end())
    }

    pub fn view(state: &AppSwitcher) -> Element<'_, Message> {
        let tv = &state.theme;
        let (surface_w, surface_h) = surface_size(state.toplevels.len(), tv);

        if !state.ready || state.finishing {
            return container(Space::new())
                .width(Length::Fixed(surface_w as f32))
                .height(Length::Fixed(surface_h as f32))
                .into();
        }

        let bg = Color::from_rgba(tv.bg[0], tv.bg[1], tv.bg[2], tv.bg[3]);
        let sel_bg = Color::from_rgba(
            tv.selected_bg[0],
            tv.selected_bg[1],
            tv.selected_bg[2],
            tv.selected_bg[3],
        );
        let corner = tv.corner_radius;
        let icon_sz = tv.icon_size;
        let cell_px = f32::from(icon_sz + 2 * CELL_PAD);

        let mut selected_label = String::new();

        let cells: Vec<Element<Message>> = state
            .visuals
            .iter()
            .enumerate()
            .map(|(i, visual)| {
                let selected = i == state.selected;
                if selected {
                    selected_label = visual.label.clone();
                }

                let app_icon: Element<Message> = match &visual.icon {
                    AppIcon::Named(name) => icon::from_name(name.as_str())
                        .size(icon_sz)
                        .icon()
                        .size(icon_sz)
                        .width(Length::Fixed(f32::from(icon_sz)))
                        .height(Length::Fixed(f32::from(icon_sz)))
                        .into(),
                    AppIcon::File(path) => icon::icon(icon::from_path(path.clone()))
                        .size(icon_sz)
                        .width(Length::Fixed(f32::from(icon_sz)))
                        .height(Length::Fixed(f32::from(icon_sz)))
                        .into(),
                };

                let cell = container(app_icon)
                    .width(Length::Fixed(cell_px))
                    .height(Length::Fixed(cell_px))
                    .center_x(Length::Fixed(cell_px))
                    .center_y(Length::Fixed(cell_px))
                    .padding(CELL_PAD)
                    .style(move |_: &cosmic::Theme| {
                        if selected {
                            ContainerStyle {
                                background: Some(Background::Color(sel_bg)),
                                border: Border {
                                    radius: (corner - 2.0).into(),
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                },
                                ..Default::default()
                            }
                        } else {
                            ContainerStyle::default()
                        }
                    });

                mouse_area(cell)
                    .on_press(Message::SelectIndex(i))
                    .on_release(Message::Activate)
                    .into()
            })
            .collect();

        let fg = Color::from_rgba(tv.fg[0], tv.fg[1], tv.fg[2], tv.fg[3]);
        let title = text(fit_title(&selected_label, surface_w))
            .size(TITLE_SIZE)
            .height(TITLE_HEIGHT)
            .class(cosmic::theme::Text::Color(fg));

        let strip_w = surface_w.saturating_sub(SURFACE_MARGIN) as f32;
        let strip_h = surface_h.saturating_sub(SURFACE_MARGIN) as f32;

        let strip = container(
            column![
                row(cells).spacing(CELL_SPACING).align_y(Alignment::Center),
                title,
            ]
            .spacing(TITLE_GAP)
            .align_x(Alignment::Center),
        )
        .width(Length::Fixed(strip_w))
        .height(Length::Fixed(strip_h))
        .style(move |_: &cosmic::Theme| ContainerStyle {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: corner.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            shadow: Shadow {
                color: Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.6,
                },
                offset: Vector::new(0.0, 8.0),
                blur_radius: 32.0,
            },
            ..Default::default()
        })
        .padding([STRIP_PAD_Y, STRIP_PAD_X]);

        container(strip)
            .width(Length::Fixed(surface_w as f32))
            .height(Length::Fixed(surface_h as f32))
            .center_x(Length::Fixed(surface_w as f32))
            .center_y(Length::Fixed(surface_h as f32))
            .into()
    }
}

// ============================================================================
// MODULE: wayland
// ============================================================================
mod wayland {
    use anyhow::{anyhow, Context, Result};
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
    use serde::{Deserialize, Serialize};
    use std::{
        io::{Read, Write},
        os::{
            fd::AsRawFd,
            unix::{
                fs::PermissionsExt,
                net::{UnixListener, UnixStream},
            },
        },
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use switcher_config::WorkspaceScope;
    use wayland_client::{
        event_created_child,
        globals::{registry_queue_init, GlobalListContents},
        protocol::{wl_output, wl_registry, wl_seat},
        Connection, Dispatch, QueueHandle,
    };

    use crate::mru::History;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToplevelEntry {
        pub app_id: String,
        pub title: String,
        pub is_active: bool,
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
        let mut stream = UnixStream::connect(daemon_socket_path()?).context(
            "MRU service is not running. Run: systemctl --user restart cosmic-ext-app-switcher-mru.service",
        )?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        serde_json::to_writer(&mut stream, request)?;
        stream.shutdown(std::net::Shutdown::Write)?;
        let mut bytes = Vec::new();
        stream.take(4 * 1024 * 1024).read_to_end(&mut bytes)?;
        let reply: Response = serde_json::from_slice(&bytes)?;
        if let Response::Error(message) = &reply {
            return Err(anyhow!("{message}"));
        }
        Ok(reply)
    }

    pub fn snapshot(scope: WorkspaceScope) -> Result<Snapshot> {
        match request(&Request::List(scope))? {
            Response::List(snapshot) => Ok(snapshot),
            _ => Err(anyhow!("Unexpected MRU service response")),
        }
    }

    pub fn activate(generation: &str, key: u64) -> Result<()> {
        match request(&Request::Activate {
            generation: generation.into(),
            key,
        })? {
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

    const TOPLEVEL_INFO_VERSION: u32 = 1;
    const TOPLEVEL_MANAGER_VERSION: u32 = 1;
    const SEAT_VERSION: u32 = 7;

    const REQUIRED_GLOBALS: &[(&str, u32)] = &[
        ("zcosmic_toplevel_info_v1", TOPLEVEL_INFO_VERSION),
        ("zcosmic_toplevel_manager_v1", TOPLEVEL_MANAGER_VERSION),
        ("zwlr_layer_shell_v1", 1),
        ("wl_seat", SEAT_VERSION),
    ];

    pub struct GlobalStatus {
        interface: &'static str,
        needed: u32,
        advertised: Option<u32>,
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
                None => write!(f, "{interface}: NOT offered (needs v{needed})"),
            }
        }
    }

    fn required_globals_status(advertised: &[(String, u32)]) -> Vec<GlobalStatus> {
        REQUIRED_GLOBALS
            .iter()
            .map(|&(interface, needed)| GlobalStatus {
                interface,
                needed,
                advertised: advertised
                    .iter()
                    .filter(|(i, _)| i == interface)
                    .map(|&(_, v)| v)
                    .max(),
            })
            .collect()
    }

    pub fn probe_required_globals() -> Result<Vec<GlobalStatus>> {
        let conn = Connection::connect_to_env()
            .context("could not connect to the Wayland compositor (is COSMIC running?)")?;
        let (globals, _queue) = registry_queue_init::<RegistryProbe>(&conn)?;
        let advertised: Vec<(String, u32)> = globals
            .contents()
            .clone_list()
            .into_iter()
            .map(|g| (g.interface, g.version))
            .collect();
        Ok(required_globals_status(&advertised))
    }

    struct AppData {
        history: History,
        next_key: u64,
        generation: String,
        toplevels: Vec<Toplevel>,
        _info: Option<ZcosmicToplevelInfoV1>,
        manager: Option<ZcosmicToplevelManagerV1>,
        seat: Option<wl_seat::WlSeat>,
        _workspace_manager: Option<ZcosmicWorkspaceManagerV1>,
        outputs: Vec<wl_output::WlOutput>,
        toplevel_info_name: Option<u32>,
        globals: Vec<(String, u32)>,
    }

    struct Toplevel {
        key: u64,
        handle: ZcosmicToplevelHandleV1,
        app_id: String,
        title: String,
        is_active: bool,
        outputs: Vec<wl_output::WlOutput>,
        workspaces: Vec<ZcosmicWorkspaceHandleV1>,
    }

    pub fn run_daemon() -> Result<()> {
        let _lock = crate::file_lock("cosmic-ext-app-switcher-mru.lock", true)?;
        let conn = Connection::connect_to_env()?;
        let display = conn.display();
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();

        let registry = display.get_registry(&qh, ());

        let mut data = AppData {
            history: History::default(),
            next_key: 0,
            generation: format!(
                "{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ),
            toplevels: vec![],
            _info: None,
            manager: None,
            seat: None,
            _workspace_manager: None,
            outputs: vec![],
            toplevel_info_name: None,
            globals: vec![],
        };

        queue.roundtrip(&mut data)?;

        let missing: Vec<String> = required_globals_status(&data.globals)
            .iter()
            .filter(|s| !s.ok())
            .map(ToString::to_string)
            .collect();
        if !missing.is_empty() {
            return Err(anyhow!(
                "cosmic-ext-app-switcher can't run: missing Wayland interfaces ({}).",
                missing.join("; "),
            ));
        }

        queue.roundtrip(&mut data)?;

        if let Some(name) = data.toplevel_info_name.take() {
            data._info = Some(registry.bind::<ZcosmicToplevelInfoV1, _, _>(
                name,
                TOPLEVEL_INFO_VERSION,
                &qh,
                (),
            ));
        }

        for _ in 0..5 {
            queue.roundtrip(&mut data)?;
        }

        let path = daemon_socket_path()?;
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let _cleanup = crate::SocketCleanup(path);
        log::info!("MRU service ready; history starts now");

        loop {
            queue.dispatch_pending(&mut data)?;
            conn.flush()?;
            let Some(guard) = queue.prepare_read() else {
                continue;
            };
            let mut fds = [
                libc::pollfd {
                    fd: conn.backend().poll_fd().as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: listener.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
            if ready < 0 {
                drop(guard);
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error.into());
            }
            if fds[0].revents != 0 {
                guard.read()?;
                queue.dispatch_pending(&mut data)?;
            } else {
                drop(guard);
            }
            if fds[1].revents & libc::POLLIN != 0 {
                let (mut stream, _) = listener.accept()?;
                stream.set_read_timeout(Some(Duration::from_millis(500)))?;
                stream.set_write_timeout(Some(Duration::from_millis(500)))?;
                let mut bytes = Vec::new();
                if (&mut stream).take(65536).read_to_end(&mut bytes).is_err() {
                    continue;
                }
                let reply = match serde_json::from_slice::<Request>(&bytes) {
                    Ok(Request::List(scope)) => {
                        queue.roundtrip(&mut data)?;
                        Response::List(data.snapshot(scope))
                    }
                    Ok(Request::Activate { generation, key }) => {
                        queue.roundtrip(&mut data)?;
                        if generation != data.generation {
                            Response::Error("MRU service restarted; reopen Alt+Tab".into())
                        } else if let Some(t) = data.toplevels.iter().find(|t| t.key == key) {
                            if let (Some(manager), Some(seat)) = (&data.manager, &data.seat) {
                                manager.activate(&t.handle, seat);
                                queue.roundtrip(&mut data)?;
                                Response::Ok
                            } else {
                                Response::Error("Activation protocol unavailable".into())
                            }
                        } else {
                            Response::Error("Selected window has already closed".into())
                        }
                    }
                    Ok(Request::Ping) => Response::Ok,
                    Err(error) => Response::Error(error.to_string()),
                };
                let _ = serde_json::to_writer(&mut stream, &reply);
                let _ = stream.flush();
            }
        }
    }

    impl AppData {
        fn snapshot(&self, scope: WorkspaceScope) -> Snapshot {
            let reference = self
                .toplevels
                .iter()
                .filter(|t| !t.app_id.is_empty() || !t.title.is_empty())
                .min_by_key(|t| self.history.sort_key(t.key));
            let active_outputs = reference.map(|t| t.outputs.as_slice()).unwrap_or(&[]);
            let mut entries: Vec<_> = self
                .toplevels
                .iter()
                .filter(|t| !t.app_id.is_empty() || !t.title.is_empty())
                .filter(|t| match scope {
                    WorkspaceScope::AllWorkspaces => true,
                    WorkspaceScope::CurrentWorkspace => true,
                    WorkspaceScope::CurrentOutput => {
                        active_outputs.is_empty()
                            || t.outputs.iter().any(|o| active_outputs.contains(o))
                    }
                })
                .map(|t| ToplevelEntry {
                    app_id: t.app_id.clone(),
                    title: t.title.clone(),
                    is_active: t.is_active,
                    handle_key: t.key,
                })
                .collect();
            entries.sort_by_key(|t| self.history.sort_key(t.handle_key));
            Snapshot {
                generation: self.generation.clone(),
                toplevels: entries,
            }
        }
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for AppData {
        fn event(
            data: &mut Self,
            registry: &wl_registry::WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            qh: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
            {
                data.globals.push((interface.clone(), version));
                match interface.as_str() {
                    "zcosmic_toplevel_info_v1" if version >= TOPLEVEL_INFO_VERSION => {
                        data.toplevel_info_name = Some(name);
                    }
                    "zcosmic_toplevel_manager_v1" if version >= TOPLEVEL_MANAGER_VERSION => {
                        data.manager = Some(registry.bind::<ZcosmicToplevelManagerV1, _, _>(
                            name,
                            TOPLEVEL_MANAGER_VERSION,
                            qh,
                            (),
                        ));
                    }
                    "wl_seat" if version >= SEAT_VERSION => {
                        if data.seat.is_none() {
                            data.seat =
                                Some(registry.bind::<wl_seat::WlSeat, _, _>(name, SEAT_VERSION, qh, ()));
                        }
                    }
                    "wl_output" => {
                        data.outputs
                            .push(registry.bind::<wl_output::WlOutput, _, _>(name, 1, qh, ()));
                    }
                    "zcosmic_workspace_manager_v1" => {
                        data._workspace_manager =
                            Some(registry.bind::<ZcosmicWorkspaceManagerV1, _, _>(name, 1, qh, ()));
                    }
                    _ => {}
                }
            }
        }
    }

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
                    handle: toplevel,
                    app_id: String::new(),
                    title: String::new(),
                    is_active: false,
                    outputs: vec![],
                    workspaces: vec![],
                });
            }
        }

        event_created_child!(AppData, ZcosmicToplevelInfoV1, [
            zcosmic_toplevel_info_v1::EVT_TOPLEVEL_OPCODE =>
                (ZcosmicToplevelHandleV1, ()),
        ]);
    }

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

    struct RegistryProbe;

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for RegistryProbe {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZcosmicToplevelManagerV1, ()> for AppData {
        fn event(
            _: &mut Self,
            _: &ZcosmicToplevelManagerV1,
            _: zcosmic_toplevel_manager_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<wl_seat::WlSeat, ()> for AppData {
        fn event(
            _: &mut Self,
            _: &wl_seat::WlSeat,
            _: wl_seat::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<wl_output::WlOutput, ()> for AppData {
        fn event(
            _: &mut Self,
            _: &wl_output::WlOutput,
            _: wl_output::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZcosmicWorkspaceManagerV1, ()> for AppData {
        fn event(
            _: &mut Self,
            _: &ZcosmicWorkspaceManagerV1,
            _: zcosmic_workspace_manager_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }

        event_created_child!(AppData, ZcosmicWorkspaceManagerV1, [
            zcosmic_workspace_manager_v1::EVT_WORKSPACE_GROUP_OPCODE =>
                (ZcosmicWorkspaceGroupHandleV1, ()),
        ]);
    }

    impl Dispatch<ZcosmicWorkspaceGroupHandleV1, ()> for AppData {
        fn event(
            _: &mut Self,
            _: &ZcosmicWorkspaceGroupHandleV1,
            _: zcosmic_workspace_group_handle_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }

        event_created_child!(AppData, ZcosmicWorkspaceGroupHandleV1, [
            zcosmic_workspace_group_handle_v1::EVT_WORKSPACE_OPCODE =>
                (ZcosmicWorkspaceHandleV1, ()),
        ]);
    }

    impl Dispatch<ZcosmicWorkspaceHandleV1, ()> for AppData {
        fn event(
            _: &mut Self,
            _: &ZcosmicWorkspaceHandleV1,
            _: zcosmic_workspace_handle_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
}