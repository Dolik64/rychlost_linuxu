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
use switcher_config::{Theme, ThemeValues};

pub struct AppSwitcher {
    core:          Core,
    pub toplevels: Vec<ToplevelEntry>,
    pub selected:  usize,
    generation:    String,
    finishing:     bool,
    super_held:    bool,
    alt_held:      bool,
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
    KeyEvent(keyboard::Event),
}

impl Application for AppSwitcher {
    type Executor = cosmic::executor::Default;
    type Flags    = (Vec<ToplevelEntry>, bool, String, Theme);
    type Message  = Message;

    const APP_ID: &'static str = "io.github.cosmic-ext-app-switcher";

    fn core(&self) -> &Core { &self.core }
    fn core_mut(&mut self) -> &mut Core { &mut self.core }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Message>) {
        let (toplevels, reverse, generation, theme_preset) = flags;
        let n = toplevels.len();
        let selected = if reverse {
            n.saturating_sub(1)
        } else {
            1.min(n.saturating_sub(1))
        };

        let theme = theme_preset.values();
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

        (
            AppSwitcher {
                core,
                toplevels,
                selected,
                generation,
                finishing: false,
                super_held: false,
                alt_held: true,
                surface_id,
                theme,
            },
            layer_task,
        )
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        if self.finishing { return Task::none(); }
        let n = self.toplevels.len();
        if n == 0 { return iced::exit(); }
        match msg {
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
                self.finishing = true;
                if let Err(error) = crate::wayland::activate(
                    &self.generation, self.toplevels[self.selected].handle_key,
                ) {
                    log::error!("Could not activate selected window: {error:#}");
                }
                return Task::batch([
                    destroy_layer_surface(self.surface_id),
                    iced::exit(),
                ]);
            }
            Message::Cancel => {
                self.finishing = true;
                return Task::batch([
                    destroy_layer_surface(self.surface_id),
                    iced::exit(),
                ]);
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
                    self.super_held = mods.logo();
                    self.alt_held = mods.alt();
                }
            },
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let key_sub = event::listen_with(|event, _status, _window| {
            match event {
                Event::Keyboard(ke) => Some(Message::KeyEvent(ke)),
                Event::Window(iced::window::Event::Unfocused) => Some(Message::Cancel),
                _ => None,
            }
        });

        let socket_sub = {
            use std::any::TypeId;
            use cosmic::iced::futures::SinkExt;
            struct SocketSub;
            Subscription::run_with(TypeId::of::<SocketSub>(), |_| {
                cosmic::iced::stream::channel(16, |mut tx: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                    use tokio::io::AsyncReadExt;
                    let listener = tokio::net::UnixListener::from_std(
                        crate::UI_LISTENER.get().expect("UI socket initialized").try_clone().expect("clone UI socket")
                    ).expect("nonblocking UI socket");
                    loop {
                        if let Ok((mut stream, _)) = listener.accept().await {
                            let mut buf = Vec::new();
                            let read = tokio::time::timeout(std::time::Duration::from_secs(1),
                                (&mut stream).take(16).read_to_end(&mut buf)).await;
                            if !matches!(read, Ok(Ok(_))) { continue; }
                            let msg = match buf.as_slice() {
                                b"next" => Message::SelectNext,
                                b"prev" => Message::SelectPrev,
                                _ => continue,
                            };
                            let _ = tx.send(msg).await;
                        }
                    }
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

pub fn run(
    toplevels: Vec<ToplevelEntry>,
    reverse: bool,
    generation: String,
    theme: Theme,
) -> Result<()> {
    let settings = Settings::default().no_main_window(true);
    cosmic::app::run::<AppSwitcher>(settings, (toplevels, reverse, generation, theme))
        .map_err(|e| anyhow::anyhow!("{e:?}"))
}
