// SPDX-License-Identifier: GPL-3.0-or-later
//! "Create New Room" (components/chat/NewChatModal.tsx): a direct message, a private group, or —
//! for accounts allowed to — a public channel. The room's key is made and wrapped by the chat
//! page; this is the form.

use iced::widget::{button, center, checkbox, column, container, row, space, svg, text, text_input, Column};
use iced::{Alignment, Color, Fill, Shadow, Vector};

use crate::components::overlay;
use crate::components::ui::{icon, El};
use crate::app::Message;
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Dm,
    Group,
    Public,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cipher {
    #[default]
    Aes256Gcm,
    ChaCha20,
}

impl Cipher {
    pub fn id(self) -> &'static str {
        match self {
            Cipher::Aes256Gcm => "aes-256-gcm",
            Cipher::ChaCha20 => "chacha20-poly1305",
        }
    }
}

pub const AUTO_BURN: [&str; 5] = ["off", "30s", "5m", "1h", "24h"];

#[derive(Debug, Clone)]
pub enum NewChatMsg {
    Kind(Kind),
    DmUsername(String),
    Usernames(String),
    Title(String),
    Topic(String),
    MaxMembers(String),
    ToggleAdvanced,
    Cipher(Cipher),
    AutoBurn(&'static str),
    AntiCopy(bool),
    Submit,
    Close,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub kind: Kind,
    pub can_create_public: bool,
    pub dm_username: String,
    pub usernames: String,
    pub title: String,
    pub topic: String,
    pub max_members: String,
    pub advanced: bool,
    pub cipher: Cipher,
    pub auto_burn: &'static str,
    pub anti_copy: bool,
    pub busy: bool,
    pub error: Option<String>,
}

impl State {
    pub fn new(can_create_public: bool) -> Self {
        Self { can_create_public, auto_burn: "off", ..Default::default() }
    }

    pub fn update(&mut self, msg: NewChatMsg) {
        self.error = None;
        match msg {
            NewChatMsg::Kind(k) => self.kind = k,
            NewChatMsg::DmUsername(v) => self.dm_username = v,
            NewChatMsg::Usernames(v) => self.usernames = v,
            NewChatMsg::Title(v) => self.title = v,
            NewChatMsg::Topic(v) => self.topic = v.chars().take(500).collect(),
            NewChatMsg::MaxMembers(v) => self.max_members = v.chars().filter(char::is_ascii_digit).take(3).collect(),
            NewChatMsg::ToggleAdvanced => self.advanced = !self.advanced,
            NewChatMsg::Cipher(c) => self.cipher = c,
            NewChatMsg::AutoBurn(t) => self.auto_burn = t,
            NewChatMsg::AntiCopy(v) => self.anti_copy = v,
            NewChatMsg::Submit | NewChatMsg::Close => {}
        }
    }

    /// The group's members as typed: "dazay, @arbiter" → ["dazay", "arbiter"].
    pub fn member_usernames(&self) -> Vec<String> {
        self.usernames.split([',', '\n']).map(|u| u.trim().trim_start_matches('@').to_string()).filter(|u| !u.is_empty()).collect()
    }

    pub fn dm_target(&self) -> String {
        self.dm_username.trim().trim_start_matches('@').to_string()
    }
}

struct Meta {
    label: &'static str,
    subtitle: &'static str,
    color: Color,
    bg: Color,
    from: Color,
    to: Color,
    shadow: Color,
}

fn meta(kind: Kind) -> Meta {
    match kind {
        Kind::Dm => Meta { label: "Direct Message", subtitle: "Fixed 1-on-1 E2EE Conversation", color: INDIGO_ICON, bg: rgba(0x6366f1, 0.2), from: INDIGO, to: INDIGO_DARK, shadow: rgba(0x6366f1, 0.35) },
        Kind::Group => Meta { label: "Group", subtitle: "Private E2EE Channel with Multiple Members", color: hex(0xa78bfa), bg: rgba(0xa78bfa, 0.2), from: hex(0x8b5cf6), to: hex(0x6d28d9), shadow: rgba(0x8b5cf6, 0.35) },
        Kind::Public => Meta { label: "Public", subtitle: "Open Public Discussion", color: SKY, bg: rgba(0x38bdf8, 0.2), from: hex(0x3b82f6), to: hex(0x2563eb), shadow: rgba(0x3b82f6, 0.35) },
    }
}

fn kind_icon(kind: Kind) -> svg::Handle {
    match kind {
        Kind::Dm => icons::lock(),
        Kind::Group => icons::users(),
        Kind::Public => icons::globe(),
    }
}

fn filled(m: &Meta, radius: f32) -> impl Fn(&iced::Theme, button::Status) -> button::Style + use<> {
    let (from, to, shadow) = (m.from, m.to, m.shadow);
    move |_, status| button::Style {
        background: Some(if matches!(status, button::Status::Disabled) { rgba(0xffffff, 0.06).into() } else { gradient(from, to) }),
        text_color: if matches!(status, button::Status::Disabled) { FAINT } else { Color::WHITE },
        border: border(Color::TRANSPARENT, 0.0, radius),
        shadow: Shadow { color: shadow, offset: Vector::new(0.0, 4.0), blur_radius: 12.0 },
        ..Default::default()
    }
}

fn label(s: &str) -> El<'_> {
    text(s).size(12).font(SEMIBOLD).color(TEXT_SECONDARY).into()
}

fn input<'a>(placeholder: &'a str, value: &'a str, on: fn(String) -> NewChatMsg) -> El<'a> {
    text_input(placeholder, value)
        .size(13)
        .padding([11, 14])
        .style(field)
        .on_input(move |v| Message::Chat(ChatMsg::NewChat(on(v))))
        .on_submit(Message::Chat(ChatMsg::NewChat(NewChatMsg::Submit)))
        .into()
}

fn chip<'a>(content: El<'a>, active: bool, active_border: Color, active_bg: Color, on: NewChatMsg) -> El<'a> {
    button(content)
        .width(Fill)
        .padding([7, 8])
        .style(move |_, status| button::Style {
            background: Some(if active { active_bg } else if matches!(status, button::Status::Hovered) { rgba(0xffffff, 0.05) } else { rgba(0xffffff, 0.02) }.into()),
            text_color: if active { Color::WHITE } else { MUTED },
            border: border(if active { active_border } else { rgba(0xffffff, 0.08) }, 1.0, 8.0),
            ..Default::default()
        })
        .on_press(Message::Chat(ChatMsg::NewChat(on)))
        .into()
}

fn advanced(state: &State) -> El<'_> {
    let m = |v| Message::Chat(ChatMsg::NewChat(v));
    let toggle = button(
        row![
            icon(icons::shield_check(), 14, SKY),
            text("Advanced E2EE & Privacy Settings").size(12).font(BOLD).color(INDIGO_SOFT),
            space().width(Fill),
            icon(if state.advanced { icons::chevron_up() } else { icons::chevron_down() }, 15, INDIGO_SOFT),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([10, 14])
    .style(ghost(12.0, rgba(0x6366f1, 0.06)))
    .on_press(m(NewChatMsg::ToggleAdvanced));
    let mut body = column![toggle];
    if state.advanced {
        let small = |s: &'static str| text(s).size(11).font(SEMIBOLD).color(MUTED);
        let cipher = |c: Cipher, handle: svg::Handle, tint: Color, name: &'static str, edge: Color, bg: Color| {
            chip(row![icon(handle, 12, tint), text(name).size(11).font(SEMIBOLD)].spacing(5).align_y(Alignment::Center).into(), state.cipher == c, edge, bg, NewChatMsg::Cipher(c))
        };
        let mut burn = row![].spacing(4);
        for t in AUTO_BURN {
            let on = state.auto_burn == t;
            burn = burn.push(
                button(center(text(t.to_uppercase()).size(10).font(BOLD)).height(14))
                    .width(Fill)
                    .padding([5, 0])
                    .style(move |_, _| button::Style {
                        background: Some(if on { rgba(0xef4444, 0.2) } else { rgba(0xffffff, 0.02) }.into()),
                        text_color: if on { hex(0xfca5a5) } else { MUTED },
                        border: border(if on { hex(0xf87171) } else { rgba(0xffffff, 0.06) }, 1.0, 6.0),
                        ..Default::default()
                    })
                    .on_press(m(NewChatMsg::AutoBurn(t))),
            );
        }
        body = body.push(
            container(
                column![
                    column![
                        small("ENCRYPTION CIPHER:"),
                        row![
                            cipher(Cipher::Aes256Gcm, icons::cpu(), INDIGO_ICON, "AES-256-GCM", INDIGO, rgba(0x6366f1, 0.2)),
                            cipher(Cipher::ChaCha20, icons::sparkles(), SKY, "ChaCha20-Poly", SKY, rgba(0x38bdf8, 0.2)),
                        ]
                        .spacing(6),
                    ]
                    .spacing(5),
                    column![row![icon(icons::flame(), 12, DANGER_TEXT), small("AUTO-BURN (SELF-DESTRUCT MESSAGES):")].spacing(5).align_y(Alignment::Center), burn].spacing(5),
                    row![
                        icon(icons::eye_off(), 13, hex(0xa78bfa)),
                        text("Anti-Leak / Copy Protection").size(11).font(MEDIUM).color(TEXT),
                        space().width(Fill),
                        checkbox(state.anti_copy).size(14).style(tick).on_toggle(|v| Message::Chat(ChatMsg::NewChat(NewChatMsg::AntiCopy(v)))),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                ]
                .spacing(12),
            )
            .padding([10, 14]),
        );
    }
    container(body).style(boxed(rgba(0x6366f1, 0.04), rgba(0x6366f1, 0.25), 12.0)).into()
}

pub fn view(state: &State) -> El<'_> {
    let m = |v| Message::Chat(ChatMsg::NewChat(v));
    let me = meta(state.kind);
    let header_icon = match state.kind {
        Kind::Dm => icons::shield(),
        Kind::Group => icons::users(),
        Kind::Public => icons::globe(),
    };
    let header = row![
        container(center(icon(header_icon, 18, me.color))).width(34).height(34).style(boxed(me.bg, Color::TRANSPARENT, 10.0)),
        column![text("Create New Room").size(16).font(BOLD).color(Color::WHITE), text(me.subtitle).size(11).color(TEXT_SECONDARY)].spacing(1),
        space().width(Fill),
        button(icon(icons::x(), 18, MUTED)).padding(4).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(m(NewChatMsg::Close)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let mut tabs = row![].spacing(4);
    for k in [Kind::Dm, Kind::Group, Kind::Public] {
        if k == Kind::Public && !state.can_create_public {
            continue;
        }
        let km = meta(k);
        let active = state.kind == k;
        let content = row![icon(kind_icon(k), 14, Color::WHITE), text(km.label).size(12).font(BOLD)].spacing(5).align_y(Alignment::Center);
        let b = button(center(content).height(18)).width(Fill).padding([9, 4]).on_press(m(NewChatMsg::Kind(k)));
        tabs = tabs.push(if active { b.style(filled(&km, 9.0)) } else { b.style(ghost(9.0, rgba(0xffffff, 0.04))) });
    }

    let mut form: Column<'_, Message> = column![header, container(tabs).padding(4).style(boxed(rgba(0xffffff, 0.04), Color::TRANSPARENT, 12.0))].spacing(16);
    match state.kind {
        Kind::Dm => {
            form = form.push(column![label("Username:"), input("e.g. dazay", &state.dm_username, NewChatMsg::DmUsername)].spacing(6)).push(advanced(state));
        }
        Kind::Group => {
            let ghost_tag = container(text("Leave empty for Ghost room").size(10).color(INDIGO_ICON)).padding([2, 6]).style(boxed(rgba(0x6366f1, 0.15), Color::TRANSPARENT, 4.0));
            form = form
                .push(
                    column![
                        row![label("Member Usernames (Optional):"), space().width(Fill), ghost_tag].align_y(Alignment::Center),
                        input("e.g. dazay, arbiter (or leave empty)", &state.usernames, NewChatMsg::Usernames)
                    ]
                    .spacing(6),
                )
                .push(column![label("Chat Title (Optional):"), input("e.g. OTC Alpha Desk, Secret Escrow", &state.title, NewChatMsg::Title)].spacing(6))
                .push(column![label("Topic (Optional):"), input("What is this group about?", &state.topic, NewChatMsg::Topic)].spacing(6))
                .push(column![label("Max Members (Optional):"), input("Unlimited", &state.max_members, NewChatMsg::MaxMembers)].spacing(6))
                .push(advanced(state));
        }
        Kind::Public => {
            form = form
                .push(column![label("Channel Name:"), input("e.g. Trading Alpha, General Talk", &state.title, NewChatMsg::Title)].spacing(6))
                .push(column![label("Topic (Optional):"), input("What is this channel about?", &state.topic, NewChatMsg::Topic)].spacing(6));
        }
    }
    if let Some(e) = &state.error {
        form = form.push(text(e.clone()).size(12.5).color(DANGER_TEXT).line_height(1.45));
    }

    let (submit_icon, submit_label) = match state.kind {
        Kind::Dm => (icons::shield_check(), "Start Direct Message"),
        Kind::Group => (icons::users(), "Create Group Room"),
        Kind::Public => (icons::globe(), "Create Public Channel"),
    };
    let submit_label = if state.busy { "Creating…" } else { submit_label };
    form = form.push(
        button(center(row![icon(submit_icon, 16, Color::WHITE), text(submit_label).size(13).font(BOLD)].spacing(6).align_y(Alignment::Center)).height(20))
            .width(Fill)
            .padding(12)
            .style(filled(&me, 10.0))
            .on_press_maybe((!state.busy).then(|| m(NewChatMsg::Submit))),
    );

    overlay::modal(container(form).width(460).padding([24, 26]).style(modal), m(NewChatMsg::Close))
}
