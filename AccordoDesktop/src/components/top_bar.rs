// SPDX-License-Identifier: GPL-3.0-or-later
//! The dark top bar, which is also the window's title bar (TopBar.tsx).

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, center, column, container, opaque, pick_list, row, space, svg, text};
use iced::{Alignment, Color, Fill, Padding};

use super::desktop::{drag_area, window_controls};
use super::ui::{icon, El};
use crate::api::auth::User;
use crate::api::data::Chain;
use crate::store::app_store::SecureWallet;
use crate::app::{Message, SessionMsg, Nav};
use crate::icons;
use crate::theme::{self, *};

pub fn view<'a>(user: &'a User, profile_open: bool, chains: &'a [Chain], selected: Option<&'a Chain>, wallet: Option<&'a SecureWallet>) -> El<'a> {
    // The brand slides the icon rail away / back, as on the web desktop layout.
    let mut name = row![svg(icons::logo()).width(28).height(28), text("Accordo").font(EXTRABOLD).size(17).color(Color::WHITE)].spacing(10).align_y(Alignment::Center);
    // ACCORDO_PROFILE: which of the side-by-side test windows this is.
    if let Some(p) = crate::api::client::profile() {
        name = name.push(iced::widget::container(text(p).size(11).font(BOLD).color(AMBER_TEXT)).padding([1, 7]).style(boxed(rgba(0xf59e0b, 0.12), rgba(0xf59e0b, 0.35), 6.0)));
    }
    let brand = button(name)
    .padding([4, 6])
    .style(ghost(8.0, Color::TRANSPARENT))
    .on_press(Message::ToggleRail);

    let chain = container(
        row![
            icon(icons::globe(), 13, INDIGO_ICON),
            // Networks from the server (Web3Context loads /api/data/chains the same way).
            pick_list(chains, selected.cloned(), Message::SelectChain)
                .placeholder("Loading…")
                .text_size(12)
                .font(SEMIBOLD)
                .padding([4, 2])
                .width(150)
                .style(chain_select)
                .menu_style(chain_menu),
            button(icon(icons::settings(), 12, MUTED)).padding(2).style(ghost(4.0, rgba(0xffffff, 0.06))).on_press(Message::ChainSettings(crate::components::chain_settings::ChainSettingsMsg::Open)),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([0, 10])
    .height(32)
    .align_y(Vertical::Center)
    .style(boxed(FIELD, BORDER, 8.0));

    // TopBar.tsx: "Connect" until a wallet is picked, then its identicon, name and short address.
    let connect: El = match wallet {
        None => button(row![icon(icons::wallet(), 14, Color::WHITE), text("Connect").size(12).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center))
            .padding([8, 12])
            .style(primary)
            .on_press(Message::OpenWallet)
            .into(),
        Some(w) => {
            let a = &w.address;
            let short = if a.chars().count() > 12 { format!("{}…{}", a.chars().take(6).collect::<String>(), a.chars().skip(a.chars().count() - 4).collect::<String>()) } else { a.clone() };
            let name = strip_chain_suffix(&w.name);
            let mut line = row![super::wallet_panel::identicon(a, 16.0, 4.0)].spacing(6).align_y(Alignment::Center);
            if !name.is_empty() {
                line = line.push(text(super::ui::truncate(name, 14)).size(12).font(SEMIBOLD).color(hex(0xa5b4fc))).push(text("·").size(12).color(rgba(0xa5b4fc, 0.5)));
            }
            line = line.push(text(short).size(11.5).font(iced::Font::MONOSPACE).color(hex(0xa5b4fc)));
            iced::widget::tooltip(
                button(line).padding([7, 11]).style(|_, status| iced::widget::button::Style {
                    background: Some(rgba(0x6366f1, if matches!(status, iced::widget::button::Status::Hovered) { 0.2 } else { 0.12 }).into()),
                    text_color: hex(0xa5b4fc),
                    border: iced::Border { color: rgba(0x6366f1, 0.35), width: 1.0, radius: 8.0.into() },
                    ..Default::default()
                })
                .on_press(Message::OpenWallet),
                container(text(format!("{} ({})", w.name, a)).size(11.5)).padding([5, 8]).style(theme::popover),
                iced::widget::tooltip::Position::Bottom,
            )
            .into()
        }
    };

    let bell = button(center(icon(icons::bell(), 15, TEXT_BODY)))
        .width(34)
        .height(32)
        .padding(0)
        .style(framed)
        .on_press(Message::Navigate(Nav::Chat));

    let profile = button(
        row![
            crate::components::ui::avatar_pic(user.name(), user.picture(), 26.0, Some(INDIGO.into())),
            text(user.name()).size(12).font(SEMIBOLD).color(TEXT_BRIGHT),
            icon(icons::chevron_down(), 13, MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 3.0, right: 8.0, bottom: 3.0, left: 4.0 })
    .style(move |_, status| iced::widget::button::Style {
        background: Some(
            if profile_open || matches!(status, iced::widget::button::Status::Hovered) { rgba(0xffffff, 0.08) } else { rgba(0xffffff, 0.04) }
                .into(),
        ),
        text_color: TEXT_BRIGHT,
        border: iced::Border { color: BORDER, width: 1.0, radius: 20.0.into() },
        ..Default::default()
    })
    .on_press(Message::Session(SessionMsg::ToggleMenu));

    let bar = container(
        row![
            brand,
            space().width(Fill),
            chain,
            connect,
            bell,
            profile,
            space().width(4),
            container(space().width(1).height(22)).style(theme::rule),
            space().width(4),
            window_controls(),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .height(54)
    .padding(Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 12.0 })
    .align_y(Vertical::Center)
    .style(panel(TOPBAR));

    drag_area(bar)
}

/// The profile popover under the pill; a click anywhere else closes it.
/// `others`: the other accounts signed in on this computer, most recent first.
/// `server`: where the open account is ("accordo.me", "0.0.0.0:8000") — the same name can be on two.
pub fn profile_menu<'a>(user: &'a User, server: String, others: Vec<&'a crate::store::app_store::SavedAccount>) -> El<'a> {
    let item = |handle, label: String, danger: bool, message: Message| -> El<'a> {
        button(row![icon(handle, 13, if danger { hex(0xef4444) } else { hex(0xe2e8f0) }), text(label).size(12).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center))
            .width(Fill)
            .padding([7, 10])
            .style(menu_item(danger))
            .on_press(message)
            .into()
    };
    let subtitle = user.email.clone().unwrap_or_else(|| format!("@{}", user.username));
    let menu = container(
        column![
            row![
                crate::components::ui::avatar_pic(user.name(), user.picture(), 36.0, Some(INDIGO.into())),
                column![
                    text(user.name()).size(13).font(BOLD).color(Color::WHITE),
                    text(subtitle).size(11).color(TEXT_SECONDARY),
                    text(server).size(10.5).font(iced::Font::MONOSPACE).color(FAINT),
                ]
                .spacing(2),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            container(space().width(Fill).height(1)).style(theme::rule),
            item(icons::settings(), "Account Settings".into(), false, Message::Navigate(Nav::Settings)),
            accounts_section(others),
            container(space().width(Fill).height(1)).style(theme::rule),
            // With other accounts here, "Logout" read like switching: it says whose.
            item(icons::x(), format!("Log out of @{}", user.username), true, Message::Session(SessionMsg::Logout)),
        ]
        .spacing(10),
    )
    .width(250)
    .padding(14)
    .style(popover);

    iced::widget::mouse_area(
        container(opaque(menu))
            .width(Fill)
            .height(Fill)
            .align_x(Horizontal::Right)
            .padding(Padding { top: 50.0, right: 130.0, bottom: 0.0, left: 0.0 }),
    )
    .on_press(Message::Session(SessionMsg::ToggleMenu))
    .into()
}

/// Switch to another account signed in here, or add one.
fn accounts_section<'a>(others: Vec<&'a crate::store::app_store::SavedAccount>) -> El<'a> {
    let mut col = column![container(space().width(Fill).height(1)).style(theme::rule)].spacing(4);
    if !others.is_empty() {
        col = col.push(text("SWITCH ACCOUNT").size(10).font(BOLD).color(FAINT));
    }
    for a in others {
        let sub = if a.signed_out { format!("{} · sign in again", a.server()) } else { a.server() };
        col = col.push(
            button(
                row![
                    crate::components::ui::avatar_pic(&a.username, a.avatar.as_deref(), 26.0, Some(INDIGO.into())),
                    column![text(format!("@{}", a.username)).size(12).font(SEMIBOLD).color(Color::WHITE), text(sub).size(10.5).color(if a.signed_out { AMBER_TEXT } else { TEXT_SECONDARY })].spacing(1),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding([5, 8])
            .style(menu_item(false))
            .on_press(Message::Session(SessionMsg::SwitchTo(a.key()))),
        );
    }
    col.push(
        button(row![icon(icons::plus(), 13, hex(0xe2e8f0)), text("Add account").size(12).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center))
            .width(Fill)
            .padding([7, 10])
            .style(menu_item(false))
            .on_press(Message::Session(SessionMsg::AddAccount)),
    )
    .into()
}

/// "Main (EVM)" → "Main": the web drops the network suffix in the top bar.
fn strip_chain_suffix(name: &str) -> &str {
    let n = name.trim();
    for sfx in ["(EVM)", "(Solana)", "(TON)", "(Tron)"] {
        if n.len() > sfx.len() && n[n.len() - sfx.len()..].eq_ignore_ascii_case(sfx) {
            return n[..n.len() - sfx.len()].trim_end();
        }
    }
    n
}
