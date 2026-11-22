// SPDX-License-Identifier: GPL-3.0-or-later
//! "Wallet Portfolio" drawer (WalletModal.tsx, WalletKeyReveal.tsx, AddressIdenticon.tsx):
//! unlock with the master password, the wallet list (search, active one, right-click / ⋯ menu:
//! make active, copy, rename, show key, remove), and adding a wallet — a new EVM key or an
//! imported one, sealed with the unlocked session and saved with the account.

use iced::widget::{button, center, column, container, mouse_area, row, scrollable, space, svg, text, text_input, Column};
use iced::{Alignment, Color, Fill, Font, Length};
use zeroize::Zeroizing;

use super::overlay;
use super::ui::{icon, El};
use crate::app::{Message, Nav};
use crate::icons;
use crate::store::app_store::SecureWallet;
use crate::theme::{self, *};
use crate::wallets;

/// How long a revealed key stays on screen.
pub const HIDE_AFTER_S: u32 = 30;

/// A secret travelling in a message: never printed.
#[derive(Clone)]
pub struct Secret(pub Zeroizing<String>);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddMode {
    #[default]
    Generate,
    Import,
}

#[derive(Debug, Clone)]
pub enum WalletMsg {
    Search(String),
    StartAdd,
    Back,
    Mode(AddMode),
    Regenerate,
    Solana,
    Name(String),
    Key(String),
    Address(String),
    ToggleKey,
    CopyNewAddress,
    CopyNewKey,
    Save,
    Menu(Option<String>),
    MakeActive(String),
    CopyAddress(String),
    Rename(String),
    RenameValue(String),
    RenameCommit,
    Reveal(String),
    RevealPassword(String),
    RevealSubmit,
    RevealDone(Result<Secret, String>),
    RevealShow,
    RevealCopy,
    RevealClose,
    RevealTick,
    /// Show in / hide from the profile (others see only the shown ones).
    TogglePublic(String),
    Delete(String),
    DeleteConfirm,
    DeleteCancel,
}

pub struct Reveal {
    pub wallet_id: String,
    pub password: Zeroizing<String>,
    pub key: Option<Zeroizing<String>>,
    pub shown: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub left: u32,
}

/// The drawer's own state (the unlock form is the settings card's: same password, same timer).
#[derive(Default)]
pub struct State {
    pub search: String,
    pub adding: bool,
    pub mode: AddMode,
    pub name: String,
    pub address: String,
    pub key: Zeroizing<String>,
    pub key_visible: bool,
    pub saving: bool,
    pub menu: Option<String>,
    pub renaming: Option<String>,
    pub rename_value: String,
    pub reveal: Option<Reveal>,
    pub delete: Option<String>,
}

impl State {
    /// A new EVM key in the form (handleGenerate).
    pub fn generate(&mut self, count: usize) {
        let (address, key) = wallets::evm::generate();
        self.address = address;
        self.key = key;
        self.key_visible = false;
        self.name = format!("Wallet {} (EVM)", count + 1);
    }

    pub fn clear_form(&mut self) {
        self.adding = false;
        self.name.clear();
        self.address.clear();
        self.key = Zeroizing::default();
        self.key_visible = false;
    }
}

/// What the drawer needs from the app.
pub struct Ctx<'a> {
    pub wallets: &'a [SecureWallet],
    pub active: Option<&'a str>,
    pub has_password: bool,
    pub unlocked: bool,
    pub lock_status: Option<El<'a>>,
    pub unlock_form: El<'a>,
    pub api_key_session: bool,
    /// The addresses others see; None: the server has no such choice (no switch then).
    pub public: Option<&'a [String]>,
}

fn msg(m: WalletMsg) -> Message {
    Message::Wallet(m)
}

// ── identicon ──

fn hsl(h: f32, s: f32, l: f32) -> String {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", to(r), to(g), to(b))
}

/// A symmetric 5×5 pattern from the address (AddressIdenticon): same address, same picture.
pub fn identicon_svg(address: &str) -> String {
    let mut hex: String = address.to_lowercase().trim_start_matches("0x").chars().filter(|c| c.is_ascii_hexdigit()).collect();
    while hex.len() < 40 {
        hex.push('0');
    }
    let byte = |i: usize| u8::from_str_radix(&hex[(i * 2) % 40..(i * 2) % 40 + 2], 16).unwrap_or(0) as u32;
    let hue = ((byte(0) as f32 / 255.0) * 360.0).round();
    let hue2 = (hue as u32 + 120 + byte(1) % 90) % 360;
    let (fg, spot, bg) = (hsl(hue % 360.0, 0.7, 0.62), hsl(hue2 as f32, 0.75, 0.6), hsl(hue % 360.0, 0.35, 0.16));
    let mut rects = String::new();
    for y in 0..5 {
        for x in 0..3 {
            let v = byte(2 + y * 3 + x);
            if v % 3 == 0 {
                continue;
            }
            let c = if v % 7 == 0 { &spot } else { &fg };
            rects.push_str(&format!(r#"<rect x="{x}" y="{y}" width="1" height="1" fill="{c}"/>"#));
            if x < 2 {
                rects.push_str(&format!(r#"<rect x="{}" y="{y}" width="1" height="1" fill="{c}"/>"#, 4 - x));
            }
        }
    }
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 5 5" shape-rendering="crispEdges"><rect width="5" height="5" fill="{bg}"/>{rects}</svg>"#)
}

pub fn identicon<'a>(address: &str, size: f32, radius: f32) -> El<'a> {
    let handle = svg::Handle::from_memory(identicon_svg(address).into_bytes());
    container(svg(handle).width(size).height(size))
        .width(size)
        .height(size)
        .clip(true)
        .style(move |_| container::Style { border: border(Color::TRANSPARENT, 0.0, radius), ..Default::default() })
        .into()
}

// ── pieces ──

fn label<'a>(t: &'a str) -> El<'a> {
    text(t).size(11).color(TEXT_SECONDARY).into()
}

fn link<'a>(handle: svg::Handle, caption: &'a str, colour: Color, on: WalletMsg) -> El<'a> {
    button(row![icon(handle, 12, colour), text(caption).size(11).color(colour)].spacing(4).align_y(Alignment::Center)).padding(0).style(link_button).on_press(msg(on)).into()
}

fn mono_box<'a>(content: String) -> El<'a> {
    container(text(content).size(12).font(Font::MONOSPACE).color(TEXT_BODY)).width(Fill).padding([10, 10]).style(boxed(rgba(0x000000, 0.25), BORDER, 8.0)).into()
}

fn segment_button<'a>(handle: svg::Handle, caption: &'a str, on: bool, m: WalletMsg) -> El<'a> {
    button(center(row![icon(handle, 14, if on { Color::WHITE } else { MUTED }), text(caption).size(12).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center)).height(18))
        .width(Fill)
        .padding([7, 12])
        .style(move |_, status| button::Style {
            background: Some(if on { INDIGO.into() } else if matches!(status, button::Status::Hovered) { rgba(0xffffff, 0.05).into() } else { Color::TRANSPARENT.into() }),
            text_color: if on { Color::WHITE } else { MUTED },
            border: border(Color::TRANSPARENT, 0.0, 8.0),
            ..Default::default()
        })
        .on_press(msg(m))
        .into()
}

fn add_form<'a>(st: &'a State) -> El<'a> {
    let back = button(row![icon(icons::arrow_left(), 15, MUTED), text("Back to Wallets").size(12.5).color(MUTED)].spacing(6).align_y(Alignment::Center)).padding(0).style(link_button).on_press(msg(WalletMsg::Back));
    let tabs = container(
        row![
            segment_button(icons::sparkles(), "Generate New", st.mode == AddMode::Generate, WalletMsg::Mode(AddMode::Generate)),
            segment_button(icons::key(), "Import Key", st.mode == AddMode::Import, WalletMsg::Mode(AddMode::Import)),
        ]
        .spacing(6),
    )
    .padding(4)
    .style(boxed(rgba(0x000000, 0.3), rgba(0xffffff, 0.06), 10.0));

    let name_input = text_input(if st.mode == AddMode::Import { "Wallet Name (e.g. Operational)" } else { "Wallet Name" }, &st.name)
        .size(13)
        .padding(10)
        .style(field)
        .on_input(|v| msg(WalletMsg::Name(v)));

    let body: El = if st.mode == AddMode::Generate {
        let chain = |caption: &'a str, on: bool, m: WalletMsg| -> El<'a> {
            button(center(text(caption).size(11.5).font(SEMIBOLD).align_x(iced::alignment::Horizontal::Center)).height(28))
                .width(Fill)
                .padding([6, 8])
                .style(move |_, status| button::Style {
                    background: Some(if on { rgba(0x6366f1, 0.15).into() } else if matches!(status, button::Status::Hovered) { rgba(0xffffff, 0.05).into() } else { rgba(0xffffff, 0.02).into() }),
                    text_color: if on { Color::WHITE } else { MUTED },
                    border: border(if on { INDIGO } else { BORDER }, 1.0, 8.0),
                    ..Default::default()
                })
                .on_press(msg(m))
                .into()
        };
        let key_text = if st.key_visible { st.key.to_string() } else { "•".repeat(64) };
        column![
            column![
                row![
                    text("SELECT BLOCKCHAIN").size(10.5).color(TEXT_SECONDARY),
                    space().width(Fill),
                    button(row![icon(icons::refresh_cw(), 11, hex(0xa5b4fc)), text("Regenerate").size(10.5).color(hex(0xa5b4fc))].spacing(4).align_y(Alignment::Center))
                        .padding([3, 8])
                        .style(outline_indigo)
                        .on_press(msg(WalletMsg::Regenerate)),
                ]
                .align_y(Alignment::Center),
                row![chain("EVM (Ethereum, Base, BSC…)", true, WalletMsg::Regenerate), chain("Solana", false, WalletMsg::Solana)].spacing(6),
            ]
            .spacing(6),
            column![label("Wallet Name"), name_input].spacing(4),
            column![
                row![label("Public Address"), space().width(Fill), link(icons::copy(), "Copy", hex(0x818cf8), WalletMsg::CopyNewAddress)].align_y(Alignment::Center),
                mono_box(if st.address.is_empty() { "Generating...".into() } else { st.address.clone() }),
            ]
            .spacing(4),
            column![
                row![
                    icon(icons::key(), 12, AMBER),
                    text("Private Key").size(11).font(SEMIBOLD).color(AMBER),
                    space().width(Fill),
                    link(if st.key_visible { icons::eye_off() } else { icons::eye() }, if st.key_visible { "Hide" } else { "Show" }, MUTED, WalletMsg::ToggleKey),
                    link(icons::copy(), "Copy", hex(0x818cf8), WalletMsg::CopyNewKey),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                container(text(key_text).size(12).font(Font::MONOSPACE).color(TEXT_BODY).wrapping(text::Wrapping::Glyph))
                    .width(Fill)
                    .padding([10, 10])
                    .style(boxed(rgba(0xf59e0b, 0.04), rgba(0xf59e0b, 0.3), 8.0)),
            ]
            .spacing(4),
            container(
                row![
                    icon(icons::shield_alert(), 16, hex(0xef4444)),
                    text("Backup required: Save this private key in a safe place. If you reset your vault, this is the only way to recover funds.").size(12).color(hex(0xfca5a5)).line_height(1.45),
                ]
                .spacing(8),
            )
            .padding([10, 12])
            .style(boxed(rgba(0xef4444, 0.08), rgba(0xef4444, 0.3), 8.0)),
            row![
                button(center(text("Cancel").size(13)).height(18)).width(Length::FillPortion(1)).padding([9, 0]).style(tone_button(Tone::Dark)).on_press(msg(WalletMsg::Back)),
                button(center(row![icon(icons::sparkles(), 14, Color::WHITE), text(if st.saving { "Saving..." } else { "Save to Vault" }).size(13).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center)).height(18))
                    .width(Length::FillPortion(2))
                    .padding([9, 0])
                    .style(tone_button(Tone::Accent))
                    .on_press_maybe((!st.saving && !st.key.is_empty()).then(|| msg(WalletMsg::Save))),
            ]
            .spacing(10),
        ]
        .spacing(14)
        .into()
    } else {
        let bad_key = !st.key.is_empty() && st.address.is_empty();
        column![
            text("Import an existing EVM private key.").size(13).color(TEXT_SECONDARY),
            column![label("Wallet Name"), name_input].spacing(4),
            column![
                label("Private Key"),
                text_input("EVM private key (0x…)", &st.key).secure(true).size(13).padding(10).style(field).on_input(|v| msg(WalletMsg::Key(v))).on_submit(msg(WalletMsg::Save)),
            ]
            .push(bad_key.then(|| text("Not an EVM private key (64 hex characters)").size(11).color(DANGER_TEXT)))
            .spacing(4),
            column![
                label("Public Address (Auto-detected)"),
                text_input("Public Address", &st.address).size(13).padding(10).font(Font::MONOSPACE).style(field).on_input(|v| msg(WalletMsg::Address(v))),
            ]
            .spacing(4),
            row![
                button(center(text("Back").size(13)).height(18)).width(Fill).padding([9, 0]).style(tone_button(Tone::Dark)).on_press(msg(WalletMsg::Back)),
                button(center(text(if st.saving { "Saving..." } else { "Save Wallet" }).size(13).font(SEMIBOLD)).height(18))
                    .width(Fill)
                    .padding([9, 0])
                    .style(tone_button(Tone::Accent))
                    .on_press_maybe((!st.saving).then(|| msg(WalletMsg::Save))),
            ]
            .spacing(10),
        ]
        .spacing(14)
        .into()
    };
    column![back, tabs, body].spacing(14).into()
}

fn menu<'a>(w: &'a SecureWallet, active: bool, api_key_session: bool, public: Option<bool>) -> El<'a> {
    let item = |handle: svg::Handle, caption: &'a str, danger: bool, m: WalletMsg| -> El<'a> {
        let colour = if danger { hex(0xf87171) } else { hex(0xe2e8f0) };
        button(row![icon(handle, 15, colour), text(caption).size(13).color(colour)].spacing(10).align_y(Alignment::Center))
            .width(Fill)
            .padding([8, 12])
            .style(ghost(6.0, rgba(0xffffff, 0.07)))
            .on_press(msg(m))
            .into()
    };
    let mut col: Column<'_, Message> = column![container(text(&w.name).size(11.5).color(TEXT_SECONDARY)).padding([6, 12])].spacing(2);
    if !active {
        col = col.push(item(icons::circle_check(), "Make active", false, WalletMsg::MakeActive(w.id.clone())));
    }
    col = col.push(item(icons::copy(), "Copy address", false, WalletMsg::CopyAddress(w.id.clone())));
    if let (Some(shown), false) = (public, api_key_session) {
        let (handle, caption) = if shown { (icons::eye_off(), "Hide from profile") } else { (icons::eye(), "Show in profile") };
        col = col.push(item(handle, caption, false, WalletMsg::TogglePublic(w.id.clone())));
    }
    if !api_key_session {
        col = col.push(item(icons::pencil(), "Rename", false, WalletMsg::Rename(w.id.clone())));
        if w.vault.parsed().is_some() {
            col = col.push(item(icons::key_round(), "Show private key", false, WalletMsg::Reveal(w.id.clone())));
        }
        col = col.push(item(icons::trash_2(), "Remove wallet", true, WalletMsg::Delete(w.id.clone())));
    }
    container(col.padding(6)).width(Fill).style(theme::popover).into()
}

fn wallet_row<'a>(st: &'a State, w: &'a SecureWallet, active: bool, api_key_session: bool, public: Option<bool>) -> El<'a> {
    let name: El = if st.renaming.as_deref() == Some(w.id.as_str()) {
        text_input("Wallet name", &st.rename_value)
            .id(RENAME_ID)
            .size(14)
            .padding([4, 8])
            .style(field)
            .on_input(|v| msg(WalletMsg::RenameValue(v)))
            .on_submit(msg(WalletMsg::RenameCommit))
            .width(200)
            .into()
    } else {
        text(&w.name).size(15).font(BOLD).color(Color::WHITE).into()
    };
    let chain = w.chain.as_deref().unwrap_or("EVM");
    let mut chain_line = row![text(chain).size(11).color(FAINT)].spacing(6).align_y(Alignment::Center);
    if public == Some(true) {
        chain_line = chain_line.push(row![icon(icons::eye(), 11, FAINT), text("In your profile").size(11).color(FAINT)].spacing(4).align_y(Alignment::Center));
    }
    let card = container(
        row![
            identicon(&w.address, 40.0, 10.0),
            column![name, text(wallets::short(&w.address)).size(12).font(Font::MONOSPACE).color(TEXT_SECONDARY), chain_line].spacing(4),
            space().width(Fill),
        ]
        .push(active.then(|| text("ACTIVE").size(12).font(BOLD).color(INDIGO_SOFT)))
        .push(button(icon(icons::ellipsis_vertical(), 15, TEXT_SECONDARY)).padding(7).style(ghost(6.0, rgba(0xffffff, 0.08))).on_press(msg(WalletMsg::Menu(Some(w.id.clone())))))
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .padding(14)
    .style(move |_| container::Style {
        background: Some(if active { rgba(0x6366f1, 0.08).into() } else { rgba(0xffffff, 0.03).into() }),
        border: border(if active { rgba(0x6366f1, 0.4) } else { BORDER }, 1.0, 12.0),
        ..Default::default()
    });
    let card = mouse_area(card).on_press(msg(WalletMsg::MakeActive(w.id.clone()))).on_right_press(msg(WalletMsg::Menu(Some(w.id.clone())))).interaction(iced::mouse::Interaction::Pointer);
    let mut col = column![card].spacing(6);
    if st.menu.as_deref() == Some(w.id.as_str()) {
        col = col.push(menu(w, active, api_key_session, public));
    }
    col.into()
}

pub const RENAME_ID: &str = "wallet-rename";

pub fn view<'a>(st: &'a State, ctx: Ctx<'a>) -> El<'a> {
    let header = container(
        row![
            column![
                row![icon(icons::wallet(), 18, INDIGO_SOFT), text("Wallet Portfolio").size(20).font(EXTRABOLD).color(Color::WHITE)].spacing(8).align_y(Alignment::Center),
                text("Manage your active payment wallets").size(13).color(TEXT_SECONDARY),
            ]
            .spacing(3),
            space().width(Fill),
            button(center(icon(icons::x(), 15, TEXT_BODY)))
                .width(30)
                .height(30)
                .padding(0)
                .style(|_, status| button::Style {
                    background: Some(rgba(0xffffff, if matches!(status, button::Status::Hovered) { 0.12 } else { 0.06 }).into()),
                    border: iced::Border { radius: 15.0.into(), ..Default::default() },
                    ..Default::default()
                })
                .on_press(Message::CloseWallet),
        ]
        .align_y(Alignment::Center),
    )
    .padding([18, 24]);

    let body: El = if !ctx.has_password {
        column![
            icon(icons::lock(), 40, AMBER_TEXT),
            text("Vault Not Found").size(18).font(BOLD).color(Color::WHITE),
            text("You need to setup a Master Password in the Settings menu before adding wallets.").size(14).color(TEXT_SECONDARY).align_x(iced::alignment::Horizontal::Center),
            space().height(6),
            button(text("Open Settings").size(13).font(SEMIBOLD)).padding([8, 16]).style(tone_button(Tone::Accent)).on_press(Message::Navigate(Nav::Settings)),
        ]
        .spacing(10)
        .align_x(Alignment::Center)
        .padding([24, 4])
        .into()
    } else if !ctx.unlocked {
        ctx.unlock_form
    } else if st.adding {
        add_form(st)
    } else {
        let q = st.search.trim().to_lowercase();
        let mut list = column![].spacing(12);
        let shown: Vec<&SecureWallet> = ctx.wallets.iter().filter(|w| w.name.to_lowercase().contains(&q) || w.address.to_lowercase().contains(&q)).collect();
        if shown.is_empty() {
            list = list.push(container(text("No wallets found.").size(14).color(TEXT_SECONDARY)).width(Fill).center_x(Fill).padding([40, 0]));
        }
        for w in shown {
            let active = ctx.active.is_some_and(|a| a == w.id || a.eq_ignore_ascii_case(&w.address));
            let public = ctx.public.map(|p| crate::api::auth::is_public(p, w.chain.as_deref(), &w.address));
            list = list.push(wallet_row(st, w, active, ctx.api_key_session, public));
        }
        let mut top = row![text_input("Search by name or address...", &st.search).size(13).padding([8, 12]).style(field).on_input(|v| msg(WalletMsg::Search(v)))].spacing(8);
        if !ctx.api_key_session {
            top = top.push(
                button(row![icon(icons::sparkles(), 14, hex(0x818cf8)), text("+ Add").size(13)].spacing(6).align_y(Alignment::Center))
                    .padding([8, 12])
                    .style(tone_button(Tone::Dark))
                    .on_press(msg(WalletMsg::StartAdd)),
            );
        }
        let mut col = column![].spacing(16);
        if let Some(status) = ctx.lock_status {
            col = col.push(status);
        }
        col = col.push(top);
        if ctx.api_key_session {
            col = col.push(
                container(text("Adding or removing wallets is locked: wallets decide where payments land, so they can only change in a password session.").size(12).color(AMBER_TEXT).line_height(1.45))
                    .padding([9, 11])
                    .style(notice),
            );
        }
        col.push(list).into()
    };

    overlay::drawer(
        column![header, container(space().width(Fill).height(1)).style(theme::rule), scrollable(container(body).padding(24).width(Fill)).height(Fill).style(thin_scroll)],
        420.0,
        Message::CloseWallet,
    )
}

/// "Show private key" (WalletKeyReveal): asks for the master password again, blurred until
/// clicked, gone after 30 seconds.
pub fn reveal_view<'a>(r: &'a Reveal, w: &'a SecureWallet) -> El<'a> {
    let head = row![
        icon(icons::key_round(), 17, AMBER_TEXT),
        text("Private key").size(16).font(BOLD).color(Color::WHITE),
        space().width(Fill),
        button(icon(icons::x(), 18, TEXT_SECONDARY)).padding(0).style(ghost(6.0, Color::TRANSPARENT)).on_press(msg(WalletMsg::RevealClose)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let who = row![text(&w.name).size(12.5).font(BOLD).color(Color::WHITE), text(" · ").size(12.5).color(TEXT_SECONDARY), text(wallets::short(&w.address)).size(12.5).font(Font::MONOSPACE).color(TEXT_SECONDARY)];
    let warn = container(
        row![
            icon(icons::triangle_alert(), 15, hex(0xfca5a5)),
            text("Anyone with this key controls the wallet and everything on it. Never paste it into a website or send it to anyone — Accordo support will never ask for it.").size(12).color(hex(0xfca5a5)).line_height(1.5),
        ]
        .spacing(8),
    )
    .padding([10, 12])
    .style(boxed(rgba(0xef4444, 0.08), rgba(0xef4444, 0.3), 8.0));

    let lower: El = match &r.key {
        None => column![
            text_input("Master password", &r.password)
                .secure(true)
                .id(REVEAL_ID)
                .size(14)
                .padding([11, 12])
                .style(field)
                .on_input(|v| msg(WalletMsg::RevealPassword(v)))
                .on_submit(msg(WalletMsg::RevealSubmit)),
        ]
        .push(r.error.as_ref().map(|e| text(e.clone()).size(12).color(DANGER_TEXT)))
        .push(
            button(center(text(if r.busy { "Checking…" } else { "Show private key" }).size(13).font(SEMIBOLD)).height(18))
                .width(Fill)
                .padding([10, 0])
                .style(tone_button(Tone::Accent))
                .on_press_maybe((!r.busy && !r.password.is_empty()).then(|| msg(WalletMsg::RevealSubmit))),
        )
        .spacing(10)
        .into(),
        Some(key) => {
            let shown: El = if r.shown {
                text(key.to_string()).size(12.5).font(Font::MONOSPACE).color(Color::WHITE).wrapping(text::Wrapping::Glyph).into()
            } else {
                center(row![icon(icons::eye(), 14, Color::WHITE), text("Click to reveal").size(12.5).font(SEMIBOLD).color(Color::WHITE)].spacing(6).align_y(Alignment::Center)).height(36).into()
            };
            column![
                mouse_area(container(shown).width(Fill).padding([12, 14]).style(boxed(rgba(0x000000, 0.35), BORDER, 8.0))).on_press(msg(WalletMsg::RevealShow)).interaction(iced::mouse::Interaction::Pointer),
                row![
                    button(row![icon(icons::copy(), 14, TEXT_BODY), text("Copy").size(13)].spacing(6).align_y(Alignment::Center)).padding([9, 14]).style(tone_button(Tone::Dark)).on_press(msg(WalletMsg::RevealCopy)),
                    space().width(Fill),
                    text(format!("Hides in {}s", r.left)).size(12).color(TEXT_SECONDARY),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(10)
            .into()
        }
    };
    overlay::modal(container(column![head, who, warn, lower].spacing(14)).width(440).padding(20).style(modal), msg(WalletMsg::RevealClose))
}

pub const REVEAL_ID: &str = "wallet-reveal-password";

/// "Remove “name”?" — a sealed key is gone for good unless saved.
pub fn delete_view<'a>(w: &'a SecureWallet) -> El<'a> {
    let sealed = w.vault.parsed().is_some();
    let mut actions = row![].spacing(8);
    if sealed {
        actions = actions.push(button(center(text("Save the key first").size(13)).height(18)).width(Fill).padding([9, 0]).style(tone_button(Tone::Dark)).on_press(msg(WalletMsg::Reveal(w.id.clone()))));
    }
    actions = actions.push(button(center(text("Remove").size(13)).height(18)).width(Fill).padding([9, 0]).style(tone_button(Tone::Danger)).on_press(msg(WalletMsg::DeleteConfirm)));
    overlay::modal(
        container(
            column![
                text(format!("Remove “{}”?", w.name)).size(16).font(BOLD).color(Color::WHITE),
                text(&w.address).size(12.5).font(Font::MONOSPACE).color(TEXT_SECONDARY),
                container(
                    text(if sealed {
                        "Its private key is deleted from your account. If you haven't saved the key (Show private key), anything still on this wallet is lost for good."
                    } else {
                        "This wallet was added by address only — removing it just stops Accordo from showing it."
                    })
                    .size(12.5)
                    .color(hex(0xfca5a5))
                    .line_height(1.5),
                )
                .padding([10, 12])
                .style(boxed(rgba(0xef4444, 0.08), rgba(0xef4444, 0.3), 8.0)),
                actions,
            ]
            .spacing(12),
        )
        .width(400)
        .padding(20)
        .style(modal),
        msg(WalletMsg::DeleteCancel),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn identicon_is_stable() {
        let a = super::identicon_svg("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        assert_eq!(a, super::identicon_svg("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"));
        assert!(a.contains("<rect"));
        assert_eq!(super::hsl(0.0, 1.0, 0.5), "#ff0000");
    }
}
