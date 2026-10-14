use anyhow::Result;
use cosmic::{
    app::{Application, Core, Settings, Task},
    iced::{
        self,
        event,
        keyboard::{self, key::Named},
        window::Id as WindowId,
        Event, Subscription,
    },
    iced::platform_specific::shell::wayland::commands::layer_surface::{
        Anchor, KeyboardInteractivity, Layer, get_layer_surface, destroy_layer_surface,
    },
    iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings,
    Element,
};
use crate::wayland::ToplevelEntry;
use crate::icons::{PreparedVisual, prepare_visual_for};
use crate::ipc::UiListener;
use crate::lifecycle::{AfterClose, Lifecycle};
use switcher_config::{Theme, ThemeValues};
use std::{sync::{Arc, Mutex}, time::Duration};

const REVEAL_DELAY: Duration = Duration::from_millis(35);

#[derive(Debug, Clone)]
pub struct Shortcut {
    reverse: bool,
    reply: Arc<Mutex<Option<tokio::sync::oneshot::Sender<bool>>>>,
}

impl Shortcut {
    fn acknowledge(&self, accepted: bool) -> bool {
        self.reply.lock().ok().and_then(|mut reply| reply.take())
            .is_some_and(|reply| reply.send(accepted).is_ok())
    }
}

pub struct AppSwitcher {
    core:          Core,
    pub toplevels: Vec<ToplevelEntry>,
    pub visuals:   Vec<PreparedVisual>,
    pub selected:  usize,
    generation:    String,
    pub lifecycle: Lifecycle,
    listener: UiListener,
    surface_id:    WindowId,
    pub theme:     ThemeValues,
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectNext,
    SelectPrev,
    SelectIndex(usize),
    Activate,
    Cancel,
    GraceElapsed,
    SurfaceOpened(WindowId),
    SurfaceClosed(WindowId),
    StartupTimeout,
    ShutdownTimeout,
    ActivationFinished(std::result::Result<(), String>),
    Shortcut(Shortcut),
    IpcFailed(String),
    KeyEvent(keyboard::Event),
}

impl Application for AppSwitcher {
    type Executor = cosmic::executor::Default;
    type Flags    = (Vec<ToplevelEntry>, bool, String, Theme, UiListener);
    type Message  = Message;

    const APP_ID: &'static str = "io.github.cosmic-ext-app-switcher";

    fn core(&self) -> &Core { &self.core }
    fn core_mut(&mut self) -> &mut Core { &mut self.core }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Message>) {
        let (toplevels, reverse, generation, theme_preset, listener) = flags;
        let n = toplevels.len();
        let selected = if reverse {
            n.saturating_sub(1)
        } else {
            1.min(n.saturating_sub(1))
        };

        let theme = theme_preset.values();
        let visuals: Vec<PreparedVisual> = toplevels
            .iter()
            .map(|entry| prepare_visual_for(&entry.app_id, &entry.title, theme.icon_size))
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
            async { tokio::time::sleep(REVEAL_DELAY).await; },
            |_| cosmic::Action::App(Message::GraceElapsed),
        );
        let startup_timeout = Task::perform(
            async { tokio::time::sleep(Duration::from_secs(3)).await; },
            |_| cosmic::Action::App(Message::StartupTimeout),
        );

        (
            AppSwitcher {
                core,
                toplevels,
                visuals,
                selected,
                generation,
                lifecycle: Lifecycle::default(),
                listener,
                surface_id,
                theme,
            },
            Task::batch([layer_task, ready_task, startup_timeout]),
        )
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        // These replies must still be processed during teardown. In particular,
        // never acknowledge a shortcut that this UI can no longer apply.
        match &msg {
            Message::Shortcut(shortcut) => {
                if shortcut.acknowledge(!self.lifecycle.is_closing()) && !self.lifecycle.is_closing() {
                    return self.update(if shortcut.reverse { Message::SelectPrev } else { Message::SelectNext });
                }
                return Task::none();
            }
            Message::SurfaceClosed(id) if *id == self.surface_id => {
                let key = match self.lifecycle.surface_closed() {
                    AfterClose::Ignore => return Task::none(),
                    AfterClose::Exit => return iced::exit(),
                    AfterClose::Activate(key) => key,
                };
                let generation = self.generation.clone();
                // The toolkit has removed the surface. The potentially slow
                // daemon request runs off the UI thread and completes before exit.
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || crate::wayland::activate(&generation, key)
                            .map_err(|error| format!("{error:#}")))
                            .await.map_err(|error| error.to_string()).and_then(|result| result)
                    },
                    |result| cosmic::Action::App(Message::ActivationFinished(result)),
                );
            }
            Message::ActivationFinished(result) => {
                if let Err(error) = result { log::error!("Could not activate selected window: {error}"); }
                return iced::exit();
            }
            Message::ShutdownTimeout if self.lifecycle.close_timed_out() => {
                log::error!("Timed out waiting for layer-surface teardown; exiting without activation");
                return iced::exit();
            }
            _ => {}
        }
        if self.lifecycle.is_closing() { return Task::none(); }
        let n = self.toplevels.len();
        if n == 0 { return iced::exit(); }
        match msg {
            Message::GraceElapsed => {
                self.lifecycle.grace_elapsed();
            }
            Message::SurfaceOpened(id) if id == self.surface_id => {
                // In the pinned Iced version, Opened is emitted on the first
                // layer-shell Configure, after its logical size is updated.
                self.lifecycle.configured();
            }
            Message::StartupTimeout if !self.lifecycle.is_configured() => {
                log::error!("Layer surface did not configure in time");
                return self.finish(None);
            }
            Message::IpcFailed(error) => {
                log::error!("UI socket listener failed: {error}");
                return self.finish(None);
            }
            Message::SelectNext => {
                self.selected = (self.selected + 1) % n;
            }
            Message::SelectPrev => {
                self.selected = (self.selected + n - 1) % n;
            }
            Message::SelectIndex(i) => {
                if i < n { self.selected = i; }
            }
            Message::Activate => {
                return self.finish(Some(self.toplevels[self.selected].handle_key));
            }
            Message::Cancel => {
                return self.finish(None);
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
                    iced::keyboard::Key::Named(Named::ArrowRight) | iced::keyboard::Key::Named(Named::ArrowDown) => {
                        return self.update(Message::SelectNext);
                    }
                    iced::keyboard::Key::Named(Named::ArrowLeft) | iced::keyboard::Key::Named(Named::ArrowUp) => {
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
                }
            },
            _ => {},
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let key_sub = event::listen_with(|event, _status, window| {
            match event {
                Event::Keyboard(ke) => Some(Message::KeyEvent(ke)),
                Event::Window(iced::window::Event::Unfocused) => Some(Message::Cancel),
                Event::Window(iced::window::Event::Opened { .. }) => Some(Message::SurfaceOpened(window)),
                Event::PlatformSpecific(iced::event::PlatformSpecific::Wayland(
                    iced::event::wayland::Event::Layer(iced::event::wayland::LayerEvent::Done, _, id)
                )) => Some(Message::SurfaceClosed(id)),
                _ => None,
            }
        });

        let socket_sub = {
            use cosmic::iced::futures::SinkExt;
            Subscription::run_with(self.listener.clone(), |source| {
                let source = source.clone();
                cosmic::iced::stream::channel(16, move |mut tx: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let listener = match source.try_clone().and_then(tokio::net::UnixListener::from_std) {
                        Ok(listener) => listener,
                        Err(error) => {
                            let _ = tx.send(Message::IpcFailed(error.to_string())).await;
                            std::future::pending::<()>().await;
                            unreachable!();
                        }
                    };
                    loop {
                        let (mut stream, _) = match listener.accept().await {
                            Ok(client) => client,
                            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                            Err(error) => {
                                let _ = tx.send(Message::IpcFailed(error.to_string())).await;
                                break;
                            }
                        };
                        let mut buf = Vec::new();
                        let read = tokio::time::timeout(Duration::from_millis(500),
                            (&mut stream).take(16).read_to_end(&mut buf)).await;
                        if !matches!(read, Ok(Ok(_))) { continue; }
                        let reverse = match buf.as_slice() { b"next" => false, b"prev" => true, _ => continue };
                        let (reply, response) = tokio::sync::oneshot::channel();
                        let shortcut = Shortcut { reverse, reply: Arc::new(Mutex::new(Some(reply))) };
                        if tx.send(Message::Shortcut(shortcut)).await.is_err() { break; }
                        if let Ok(Ok(accepted)) = tokio::time::timeout(Duration::from_secs(1), response).await {
                            let _ = tokio::time::timeout(Duration::from_millis(500),
                                stream.write_all(if accepted { b"1" } else { b"0" })).await;
                        }
                    }
                    // A permanent accept error must not turn into a tight retry loop.
                    std::future::pending::<()>().await;
                })
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

impl AppSwitcher {
    fn finish(&mut self, key: Option<u64>) -> Task<Message> {
        if !self.lifecycle.begin_close(key) { return Task::none(); }
        // Do not batch Destroy with Exit: wait for the toolkit's LayerEvent::Done.
        // This timer bounds a failed teardown; it is not a frame synchronization.
        Task::batch([
            destroy_layer_surface(self.surface_id),
            Task::perform(
                async { tokio::time::sleep(Duration::from_secs(2)).await; },
                |_| cosmic::Action::App(Message::ShutdownTimeout),
            ),
        ])
    }
}

pub fn run(
    toplevels: Vec<ToplevelEntry>,
    reverse: bool,
    generation: String,
    theme: Theme,
    listener: UiListener,
) -> Result<()> {
    let settings = Settings::default()
        .no_main_window(true)
        .exit_on_close(false)
        .transparent(true);
    cosmic::app::run::<AppSwitcher>(settings, (toplevels, reverse, generation, theme, listener))
        .map_err(|e| anyhow::anyhow!("{e:?}"))
}
