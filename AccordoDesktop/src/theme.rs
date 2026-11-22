// SPDX-License-Identifier: GPL-3.0-or-later
//! Colors, fonts and widget styles, taken from the web app's CSS (src/index.css, TopBar.tsx,
//! Sidebar.tsx, chat/ChatSidebar.tsx) so the native client looks the same.

use iced::border::Radius;
use iced::gradient::Linear;
use iced::widget::{button, container, scrollable, svg, text_input};
use iced::{font, Background, Border, Color, Font, Gradient, Radians, Shadow, Theme, Vector};

pub const fn hex(rgb: u32) -> Color {
    Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const fn rgba(rgb: u32, a: f32) -> Color {
    Color::from_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, a)
}

// ── palette ──
pub const BG_DARK: Color = hex(0x0a0b10); // --bg-dark
pub const TOPBAR: Color = hex(0x0d0e14); // rgba(13, 14, 20, 0.95)
pub const RAIL: Color = hex(0x111216); // sidebar
pub const LIST: Color = hex(0x0f1015);
pub const MAIN: Color = hex(0x0c0d12);
pub const FIELD: Color = hex(0x121316); // chain box
pub const BORDER: Color = rgba(0xffffff, 0.08); // --glass-border
pub const BORDER_STRONG: Color = rgba(0xffffff, 0.12);
pub const ACCENT: Color = hex(0x5865f2); // --accent
pub const INDIGO: Color = hex(0x6366f1);
pub const INDIGO_DARK: Color = hex(0x4f46e5);
pub const INDIGO_SOFT: Color = hex(0xa5b4fc);
pub const INDIGO_ICON: Color = hex(0x818cf8);
pub const TEXT: Color = hex(0xe0e2eb); // --text-primary
pub const TEXT_BRIGHT: Color = hex(0xf2f3f5);
pub const TEXT_BODY: Color = hex(0xdbdee1);
pub const TEXT_SECONDARY: Color = hex(0x8b92a5); // --text-secondary
pub const MUTED: Color = hex(0x94a3b8);
pub const FAINT: Color = hex(0x64748b);
pub const RED: Color = hex(0xf23f43);
pub const GREEN: Color = hex(0x34d399);
pub const CLOSE_RED: Color = hex(0xe5484d);

// ── fonts (Inter, bundled: the web app loads it from Google Fonts) ──
pub const INTER: Font = Font::with_name("Inter");
pub const MEDIUM: Font = Font { weight: font::Weight::Medium, ..INTER };
pub const SEMIBOLD: Font = Font { weight: font::Weight::Semibold, ..INTER };
pub const BOLD: Font = Font { weight: font::Weight::Bold, ..INTER };
/// Emoji in colour (the system's Noto Color Emoji; text fallback may pick a monochrome font).
pub const EMOJI: Font = Font::with_name("Noto Color Emoji");
pub const EXTRABOLD: Font = Font { weight: font::Weight::ExtraBold, ..INTER };

pub const FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../assets/fonts/Inter-ExtraBold.ttf"),
];

/// CSS `linear-gradient(135deg, a 0%, b 100%)`.
pub fn gradient(a: Color, b: Color) -> Background {
    // CSS 135deg points to the bottom-right; iced measures from the x axis, y down.
    Background::Gradient(Gradient::Linear(
        Linear::new(Radians(std::f32::consts::FRAC_PI_4)).add_stop(0.0, a).add_stop(1.0, b),
    ))
}

/// The avatar palettes of src/utils/avatar.ts, picked by the same sum-of-char-codes hash.
pub fn avatar_gradient(name: &str) -> Background {
    const PALETTES: [(u32, u32); 6] = [
        (0x5865f2, 0x4752c4),
        (0x57f287, 0x2ecc71),
        (0xfee75c, 0xe67e22),
        (0xeb459e, 0xc2185b),
        (0xed4245, 0x992d22),
        (0x38bdf8, 0x0284c7),
    ];
    let hash: u32 = name.encode_utf16().map(u32::from).sum();
    let (a, b) = PALETTES[hash as usize % PALETTES.len()];
    gradient(hex(a), hex(b))
}

pub fn border(color: Color, width: f32, radius: f32) -> Border {
    Border { color, width, radius: Radius::from(radius) }
}

// ── containers ──

pub fn panel(bg: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style { background: Some(bg.into()), ..Default::default() }
}

/// A background with a 1px line on one side is drawn as a bordered box clipped by its parent:
/// iced borders are all-around, so the top bar / panels get their separator from a 1px rule.
pub fn rule(_: &Theme) -> container::Style {
    container::Style { background: Some(BORDER.into()), ..Default::default() }
}

pub fn boxed(bg: Color, line: Color, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(bg.into()),
        border: border(line, 1.0, radius),
        ..Default::default()
    }
}

pub fn round_fill(bg: Background, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(bg),
        border: border(Color::TRANSPARENT, 0.0, radius),
        text_color: Some(Color::WHITE),
        ..Default::default()
    }
}

pub fn dashed_hint(_: &Theme) -> container::Style {
    // CSS uses a dashed border; iced has no dash style, a faint solid line reads the same.
    container::Style {
        border: border(rgba(0xa5b4fc, 0.3), 1.0, 8.0),
        ..Default::default()
    }
}

// ── buttons ──

/// Transparent until hovered (icons, rows).
pub fn ghost(radius: f32, hover: Color) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(hover.into()),
            _ => None,
        },
        text_color: TEXT,
        border: border(Color::TRANSPARENT, 0.0, radius),
        ..Default::default()
    }
}

/// Selectable list row / rail item: `active` keeps the highlight.
pub fn selectable(active: bool, active_bg: Color, radius: f32) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: if active {
            Some(active_bg.into())
        } else {
            match status {
                button::Status::Hovered | button::Status::Pressed => Some(rgba(0xffffff, 0.05).into()),
                _ => None,
            }
        },
        text_color: TEXT,
        border: border(Color::TRANSPARENT, 0.0, radius),
        shadow: if active {
            Shadow { color: rgba(0x5865f2, 0.3), offset: Vector::new(0.0, 4.0), blur_radius: 12.0 }
        } else {
            Shadow::default()
        },
        ..Default::default()
    }
}

/// Small square icon button with a frame (chat list toolbar, bell).
pub fn framed(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered | button::Status::Pressed => rgba(0xffffff, 0.09),
                _ => rgba(0xffffff, 0.05),
            }
            .into(),
        ),
        text_color: INDIGO_SOFT,
        border: border(rgba(0xffffff, 0.1), 1.0, 8.0),
        ..Default::default()
    }
}

/// Indigo gradient call to action ("Connect", "New chat").
pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    let (a, b) = match status {
        button::Status::Hovered | button::Status::Pressed => (hex(0x5b54f0), hex(0x7477f5)),
        _ => (INDIGO_DARK, INDIGO),
    };
    button::Style {
        background: Some(gradient(a, b)),
        text_color: Color::WHITE,
        border: border(Color::TRANSPARENT, 0.0, 8.0),
        shadow: Shadow { color: rgba(0x6366f1, 0.25), offset: Vector::new(0.0, 2.0), blur_radius: 8.0 },
        ..Default::default()
    }
}

/// Dark outlined button ("Public rooms").
pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered | button::Status::Pressed => rgba(0xffffff, 0.06),
                _ => hex(0x111216),
            }
            .into(),
        ),
        text_color: TEXT_BRIGHT,
        border: border(BORDER_STRONG, 1.0, 8.0),
        ..Default::default()
    }
}

/// Filter pill (All / Unread / DMs / Groups).
pub fn pill(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(
            if active {
                rgba(0x6366f1, 0.25)
            } else if matches!(status, button::Status::Hovered) {
                rgba(0xffffff, 0.07)
            } else {
                rgba(0xffffff, 0.04)
            }
            .into(),
        ),
        text_color: if active { Color::WHITE } else { MUTED },
        border: border(if active { rgba(0x6366f1, 0.6) } else { BORDER }, 1.0, 12.0),
        ..Default::default()
    }
}

/// Window controls: neutral hover, red for close.
pub fn window_control(close: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: hovered.then(|| if close { CLOSE_RED } else { rgba(0xffffff, 0.08) }.into()),
            text_color: if hovered { Color::WHITE } else { MUTED },
            border: border(Color::TRANSPARENT, 0.0, 6.0),
            ..Default::default()
        }
    }
}

// ── inputs ──

pub fn bare_input(_: &Theme, _: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Color::TRANSPARENT.into(),
        border: border(Color::TRANSPARENT, 0.0, 0.0),
        icon: MUTED,
        placeholder: FAINT,
        value: TEXT_BODY,
        selection: rgba(0x6366f1, 0.45),
    }
}

// ── svg / scroll ──

pub fn tint(color: Color) -> impl Fn(&Theme, svg::Status) -> svg::Style {
    move |_, _| svg::Style { color: Some(color) }
}

pub fn thin_scroll(_: &Theme, status: scrollable::Status) -> scrollable::Style {
    let hovered = !matches!(status, scrollable::Status::Active { .. });
    let rail = scrollable::Rail {
        background: None,
        border: border(Color::TRANSPARENT, 0.0, 0.0),
        scroller: scrollable::Scroller {
            background: rgba(0xffffff, if hovered { 0.14 } else { 0.06 }).into(),
            border: border(Color::TRANSPARENT, 0.0, 3.0),
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: FIELD.into(),
            border: border(BORDER, 1.0, 8.0),
            shadow: Shadow::default(),
            icon: MUTED,
        },
    }
}

// ── auth screens (.auth-card, input, .btn.primary, a) ──

/// `.card`: --bg-modal (rgba(20, 22, 35, 0.7)) over the page background.
pub const CARD: Color = hex(0x11131d);
pub const LINK: Color = hex(0x7c84f6);
pub const SKY: Color = hex(0x38bdf8);

pub fn card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(CARD.into()),
        border: border(BORDER, 1.0, 16.0),
        shadow: Shadow { color: rgba(0x000000, 0.35), offset: Vector::new(0.0, 12.0), blur_radius: 40.0 },
        ..Default::default()
    }
}

/// Global `input` look: dark field, glass border, accent border when focused.
pub fn field(_: &Theme, status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: hex(0x0e0f17).into(),
        border: border(
            match status {
                text_input::Status::Focused { .. } => ACCENT,
                text_input::Status::Hovered => BORDER_STRONG,
                _ => BORDER,
            },
            1.0,
            8.0,
        ),
        icon: MUTED,
        placeholder: hex(0x6b7080),
        value: TEXT,
        selection: rgba(0x5865f2, 0.45),
    }
}

/// `.btn.primary`: solid accent, darker on hover, dimmed while busy.
pub fn accent_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered | button::Status::Pressed => hex(0x4752c4),
                button::Status::Disabled => rgba(0x5865f2, 0.55),
                button::Status::Active => ACCENT,
            }
            .into(),
        ),
        text_color: if matches!(status, button::Status::Disabled) { rgba(0xffffff, 0.75) } else { Color::WHITE },
        border: border(Color::TRANSPARENT, 0.0, 8.0),
        ..Default::default()
    }
}

/// A text link: no box, the label carries the colour.
pub fn link_button(_: &Theme, _: button::Status) -> button::Style {
    button::Style { background: None, text_color: LINK, border: border(Color::TRANSPARENT, 0.0, 0.0), ..Default::default() }
}

// ── toasts (.toast-info / -success / -error) ──

pub fn toast(kind: crate::components::toast::Kind) -> impl Fn(&Theme) -> container::Style {
    use crate::components::toast::Kind;
    move |_| container::Style {
        background: Some(
            match kind {
                Kind::Info => hex(0x2b2d3a),
                Kind::Success => rgba(0x3ba55c, 0.92),
                Kind::Error => rgba(0xed4245, 0.92),
            }
            .into(),
        ),
        border: border(if kind == Kind::Info { BORDER } else { Color::TRANSPARENT }, 1.0, 8.0),
        shadow: Shadow { color: rgba(0x000000, 0.4), offset: Vector::new(0.0, 8.0), blur_radius: 24.0 },
        text_color: Some(Color::WHITE),
        ..Default::default()
    }
}

/// The profile popover (TopBar.tsx): #111214, light border, deep shadow.
pub fn popover(_: &Theme) -> container::Style {
    container::Style {
        background: Some(hex(0x111214).into()),
        border: border(rgba(0xffffff, 0.1), 1.0, 10.0),
        shadow: Shadow { color: rgba(0x000000, 0.75), offset: Vector::new(0.0, 12.0), blur_radius: 32.0 },
        ..Default::default()
    }
}

pub fn menu_item(danger: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (bg, line, fg) = if danger {
            (rgba(0xef4444, if hovered { 0.2 } else { 0.12 }), rgba(0xef4444, 0.3), hex(0xef4444))
        } else {
            (rgba(0xffffff, if hovered { 0.09 } else { 0.05 }), BORDER, hex(0xe2e8f0))
        };
        button::Style { background: Some(bg.into()), text_color: fg, border: border(line, 1.0, 6.0), ..Default::default() }
    }
}

// ── settings cards, buttons, badges (SettingsPage.tsx, components/settings/*) ──

pub const AMBER: Color = hex(0xf59e0b);
pub const AMBER_TEXT: Color = hex(0xfbbf24);
pub const DANGER_TEXT: Color = hex(0xf87171);

/// The settings `.card`; `accent` gives the indigo-bordered variant (chat encryption).
pub fn settings_card(accent: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(hex(0x10111a).into()),
        border: border(if accent { rgba(0x6366f1, 0.3) } else { BORDER }, 1.0, 16.0),
        ..Default::default()
    }
}

/// Faint inset row inside a card (Email / Username / Two-factor…).
pub fn soft_row(_: &Theme) -> container::Style {
    container::Style {
        background: Some(rgba(0xffffff, 0.02).into()),
        border: border(rgba(0xffffff, 0.05), 1.0, 10.0),
        ..Default::default()
    }
}

/// Tinted square behind a card's header icon.
pub fn icon_tile(bg: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style { background: Some(bg.into()), border: border(Color::TRANSPARENT, 0.0, 6.0), ..Default::default() }
}

/// Small pill label ("ACTIVE", "Verified", "USER").
pub fn badge(bg: Color, line: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style { background: Some(bg.into()), border: border(line, 1.0, 5.0), ..Default::default() }
}

/// Amber-framed notice (chat password not set, the "forget it" warning).
pub fn notice(_: &Theme) -> container::Style {
    container::Style {
        background: Some(rgba(0xf59e0b, 0.06).into()),
        border: border(rgba(0xf59e0b, 0.45), 1.0, 10.0),
        ..Default::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Accent,
    Dark,
    Danger,
}

/// Card buttons: solid accent, dark neutral, or red-tinted destructive.
pub fn tone_button(tone: Tone) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let disabled = matches!(status, button::Status::Disabled);
        let (bg, line, fg) = match tone {
            Tone::Accent => (
                if disabled { rgba(0x6366f1, 0.45) } else if hovered { hex(0x7377f3) } else { INDIGO },
                Color::TRANSPARENT,
                if disabled { rgba(0xffffff, 0.6) } else { Color::WHITE },
            ),
            Tone::Dark => (if hovered { hex(0x1c1d26) } else { hex(0x15161d) }, BORDER, hex(0xe2e8f0)),
            Tone::Danger => (rgba(0xef4444, if hovered { 0.22 } else { 0.12 }), rgba(0xef4444, 0.25), DANGER_TEXT),
        };
        button::Style { background: Some(bg.into()), text_color: fg, border: border(line, 1.0, 7.0), ..Default::default() }
    }
}

/// Dimmed backdrop behind modals and the wallet drawer.
pub fn backdrop(_: &Theme) -> container::Style {
    container::Style { background: Some(rgba(0x000000, 0.72).into()), ..Default::default() }
}

pub fn modal(_: &Theme) -> container::Style {
    container::Style {
        background: Some(hex(0x13141c).into()),
        border: border(rgba(0xffffff, 0.1), 1.0, 14.0),
        shadow: Shadow { color: rgba(0x000000, 0.6), offset: Vector::new(0.0, 20.0), blur_radius: 50.0 },
        ..Default::default()
    }
}

pub fn drawer(_: &Theme) -> container::Style {
    container::Style {
        background: Some(hex(0x0f1016).into()),
        border: border(BORDER, 1.0, 0.0),
        shadow: Shadow { color: rgba(0x000000, 0.6), offset: Vector::new(-12.0, 0.0), blur_radius: 36.0 },
        ..Default::default()
    }
}

/// Segmented tabs (Password | Seed phrase).
pub fn segment(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: if active {
            Some(rgba(0x6366f1, 0.28).into())
        } else if matches!(status, button::Status::Hovered) {
            Some(rgba(0xffffff, 0.04).into())
        } else {
            None
        },
        text_color: if active { Color::WHITE } else { hex(0xc7d2fe) },
        border: border(Color::TRANSPARENT, 0.0, 7.0),
        ..Default::default()
    }
}

/// Indigo outlined small button ("Generate password").
pub fn outline_indigo(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(rgba(0x6366f1, if matches!(status, button::Status::Hovered) { 0.2 } else { 0.1 }).into()),
        text_color: hex(0xc7d2fe),
        border: border(rgba(0x6366f1, 0.4), 1.0, 6.0),
        ..Default::default()
    }
}

pub fn switch(_: &Theme, status: iced::widget::toggler::Status) -> iced::widget::toggler::Style {
    use iced::widget::toggler::Status;
    let on = matches!(status, Status::Active { is_toggled: true } | Status::Hovered { is_toggled: true });
    iced::widget::toggler::Style {
        background: (if on { INDIGO } else { hex(0x2a2c36) }).into(),
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
        foreground: (if on { Color::WHITE } else { hex(0x6b7080) }).into(),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: None,
        border_radius: None,
        padding_ratio: 0.12,
    }
}

/// The network selector inside the top bar's chain box: no frame of its own.
pub fn chain_select(_: &Theme, _: iced::widget::pick_list::Status) -> iced::widget::pick_list::Style {
    iced::widget::pick_list::Style {
        text_color: TEXT_BODY,
        placeholder_color: FAINT,
        handle_color: FAINT,
        background: Color::TRANSPARENT.into(),
        border: border(Color::TRANSPARENT, 0.0, 0.0),
    }
}

pub fn chain_menu(_: &Theme) -> iced::overlay::menu::Style {
    iced::overlay::menu::Style {
        background: hex(0x1c1d22).into(),
        border: border(rgba(0xffffff, 0.1), 1.0, 8.0),
        text_color: TEXT_BODY,
        selected_text_color: Color::WHITE,
        selected_background: rgba(0x6366f1, 0.35).into(),
        shadow: Shadow { color: rgba(0x000000, 0.6), offset: Vector::new(0.0, 8.0), blur_radius: 24.0 },
    }
}

// ── finance forms (cheques, invoices, crowdfunding: components/finance/*) ──

pub const LABEL: Color = hex(0xcbd5e1);

/// Page tabs ("Write & Claim", "Incoming Cheques"…): solid indigo when active.
pub fn page_tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(
                if active {
                    hex(0x5b5ff0)
                } else if hovered {
                    hex(0x171822)
                } else {
                    hex(0x111219)
                }
                .into(),
            ),
            text_color: if active { Color::WHITE } else { hex(0xc7cbd6) },
            border: border(if active { Color::TRANSPARENT } else { BORDER }, 1.0, 8.0),
            shadow: if active { Shadow { color: rgba(0x5865f2, 0.3), offset: Vector::new(0.0, 4.0), blur_radius: 12.0 } } else { Shadow::default() },
            ..Default::default()
        }
    }
}

/// Small preset chip (fee presets, durations, categories).
pub fn chip(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(
            if active {
                rgba(0x6366f1, 0.22)
            } else if matches!(status, button::Status::Hovered) {
                rgba(0xffffff, 0.06)
            } else {
                rgba(0xffffff, 0.025)
            }
            .into(),
        ),
        text_color: if active { Color::WHITE } else { hex(0x9aa1b2) },
        border: border(if active { rgba(0x6366f1, 0.8) } else { BORDER }, 1.0, 5.0),
        ..Default::default()
    }
}

#[cfg(feature = "crowdfund")]
/// Larger selectable option card (funding model, category pills on crowdfunding).
pub fn option_card(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(
            if active {
                rgba(0x6366f1, 0.1)
            } else if matches!(status, button::Status::Hovered) {
                rgba(0xffffff, 0.04)
            } else {
                rgba(0xffffff, 0.015)
            }
            .into(),
        ),
        text_color: TEXT,
        border: border(if active { rgba(0x6366f1, 0.7) } else { BORDER }, 1.0, 8.0),
        ..Default::default()
    }
}

/// Inset panel inside a form (fee picker, expiry, "How it works").
pub fn inset(_: &Theme) -> container::Style {
    container::Style {
        background: Some(rgba(0xffffff, 0.02).into()),
        border: border(BORDER, 1.0, 8.0),
        ..Default::default()
    }
}

pub fn fee_slider(_: &Theme, _: iced::widget::slider::Status) -> iced::widget::slider::Style {
    iced::widget::slider::Style {
        rail: iced::widget::slider::Rail {
            backgrounds: (INDIGO.into(), hex(0xe5e7eb).into()),
            width: 5.0,
            border: border(Color::TRANSPARENT, 0.0, 3.0),
        },
        handle: iced::widget::slider::Handle {
            shape: iced::widget::slider::HandleShape::Circle { radius: 7.0 },
            background: INDIGO.into(),
            border_width: 2.0,
            border_color: hex(0x0c0d12),
        },
    }
}

pub fn tick(_: &Theme, status: iced::widget::checkbox::Status) -> iced::widget::checkbox::Style {
    use iced::widget::checkbox::Status;
    let checked = matches!(status, Status::Active { is_checked: true } | Status::Hovered { is_checked: true } | Status::Disabled { is_checked: true });
    iced::widget::checkbox::Style {
        background: (if checked { INDIGO } else { hex(0xf1f2f6) }).into(),
        icon_color: Color::WHITE,
        border: border(if checked { INDIGO } else { hex(0x9aa1b2) }, 1.0, 3.0),
        text_color: Some(TEXT_SECONDARY),
    }
}

/// The token selector's own frame-less pick list (inside a field-styled box).
pub fn token_select(_: &Theme, _: iced::widget::pick_list::Status) -> iced::widget::pick_list::Style {
    iced::widget::pick_list::Style {
        text_color: Color::WHITE,
        placeholder_color: TEXT_BODY,
        handle_color: MUTED,
        background: Color::TRANSPARENT.into(),
        border: border(Color::TRANSPARENT, 0.0, 0.0),
    }
}
