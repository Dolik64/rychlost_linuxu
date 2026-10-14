use cosmic::{
    iced::{
        Alignment, Border, Color, Length, Shadow, Vector,
        Background,
        widget::container::Style as ContainerStyle,
    },
    widget::{column, container, row, mouse_area, text, Space},
    widget::icon,
    Element,
};
use crate::app::{AppSwitcher, Message};
use crate::icons::AppIcon;
use switcher_config::ThemeValues;

const CELL_PAD:     u16 = 10;
const CELL_SPACING: u16 = 4;
const STRIP_PAD_X:  u16 = 18;
const STRIP_PAD_Y:  u16 = 14;
const TITLE_SIZE:   u16 = 14;
const TITLE_HEIGHT: u16 = 20;
const TITLE_GAP:    u16 = 6;
/// Breathing room around the strip, so its shadow isn't clipped by the surface edge.
const SURFACE_MARGIN: u32 = 40;

pub fn surface_size(window_count: usize, theme: &ThemeValues) -> (u32, u32) {
    let n = window_count as u32;
    let cell = u32::from(theme.icon_size) + 2 * u32::from(CELL_PAD);
    let strip_w = n * cell + n.saturating_sub(1) * u32::from(CELL_SPACING)
        + 2 * u32::from(STRIP_PAD_X);
    let strip_h = cell + u32::from(TITLE_GAP) + u32::from(TITLE_HEIGHT)
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

    let bg      = Color::from_rgba(tv.bg[0],          tv.bg[1],          tv.bg[2],          tv.bg[3]);
    let sel_bg  = Color::from_rgba(tv.selected_bg[0], tv.selected_bg[1], tv.selected_bg[2], tv.selected_bg[3]);
    let corner  = tv.corner_radius;
    let icon_sz = tv.icon_size;
    let cell_px = f32::from(icon_sz + 2 * CELL_PAD);

    let mut selected_label = String::new();

    let cells: Vec<Element<Message>> = state.visuals
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
        .align_x(Alignment::Center)
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
            color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.6 },
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

#[cfg(test)]
mod tests {
    use super::{fit_title, surface_size, TITLE_HEIGHT, TITLE_GAP};
    use switcher_config::Theme;

    #[test]
    fn long_titles_are_shortened_to_the_strip() {
        let (surface_w, _) = surface_size(3, &Theme::Dark.values());
        let long = "A window title far too long to fit under three icons at all";
        let fitted = fit_title(long, surface_w);
        assert!(fitted.ends_with('…'), "{fitted:?}");
        assert!(fitted.chars().count() < long.chars().count(), "{fitted:?}");
        assert_eq!(fit_title("Short", surface_w), "Short");
    }

    #[test]
    fn surface_leaves_room_for_the_title_row() {
        let theme = Theme::Dark.values();
        let (_, with_title) = surface_size(3, &theme);
        let icons_only = u32::from(theme.icon_size) + 20 + 28 + 40;
        assert_eq!(with_title, icons_only + u32::from(TITLE_HEIGHT) + u32::from(TITLE_GAP));
    }

    #[test]
    fn width_grows_with_each_window() {
        let theme = Theme::Dark.values();
        let (one, _) = surface_size(1, &theme);
        let (two, _) = surface_size(2, &theme);
        assert_eq!(two - one, u32::from(theme.icon_size) + 20 + 4);
    }
}
