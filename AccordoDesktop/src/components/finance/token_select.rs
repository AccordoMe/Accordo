// SPDX-License-Identifier: GPL-3.0-or-later
//! The token field of the forms: the picked token (logo, symbol, name) as a button that opens the
//! token picker window (components/token_picker.rs).

use iced::widget::{button, column, image, row, space, text};
use iced::{Alignment, Color, Fill};

use crate::api::data::Token;
use crate::app::Message;
use crate::components::token_picker::{self, Purpose, TokenPickerMsg};
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::*;

/// What the field needs besides the pick: the list is still loading / the picked token's logo.
#[derive(Clone, Default)]
pub struct Field {
    pub loading: bool,
    pub logo: Option<image::Handle>,
    /// The active wallet's balance of the picked token ("0.0421 ETH", "…" while read) and what
    /// it's worth ("≈ $123.45") when priced.
    pub balance: Option<(String, Option<String>)>,
}

fn open(p: Purpose) -> Message {
    Message::TokenPicker(TokenPickerMsg::Open(p))
}

fn field_style(_: &iced::Theme, status: button::Status) -> button::Style {
    let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(hex(0x0e0f17).into()),
        text_color: TEXT,
        border: border(if hover { BORDER_STRONG } else { BORDER }, 1.0, 8.0),
        ..Default::default()
    }
}

/// Full width: "[logo] USDC  USD Coin  ⌄" or "Select a token".
pub fn view<'a>(f: Field, selected: Option<&'a Token>, purpose: Purpose) -> El<'a> {
    let inner: El = match selected {
        Some(t) => row![
            token_picker::logo(t, f.logo.as_ref(), 24.0),
            column![text(t.symbol.clone()).size(13.5).font(BOLD).color(Color::WHITE), text(t.name.clone()).size(11).color(TEXT_SECONDARY)].spacing(0),
            space().width(Fill),
        ]
        .extend(f.balance.map(|(amount, usd)| -> El<'a> {
            let label = match usd {
                Some(usd) => row![text("Balance").size(10).color(FAINT), text(usd).size(10).color(TEXT_SECONDARY)].spacing(5),
                None => row![text("Balance").size(10).color(FAINT)],
            };
            column![label, text(amount).size(12.5).font(SEMIBOLD).color(TEXT_BODY)].spacing(0).align_x(Alignment::End).into()
        }))
        .push(icon(icons::chevron_down(), 14, MUTED))
        .spacing(10)
        .align_y(Alignment::Center)
        .into(),
        None => row![
            icon(icons::search(), 14, MUTED),
            text(if f.loading { "Loading tokens…" } else { "Select a token" }).size(13.5).color(TEXT_SECONDARY),
            space().width(Fill),
            icon(icons::chevron_down(), 14, MUTED),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into(),
    };
    button(inner).width(Fill).padding([7, 12]).style(field_style).on_press(open(purpose)).into()
}

#[cfg(feature = "crowdfund")]
/// Small, next to an amount (crowdfund): "[logo] ETH ⌄"; `fallback` when nothing is picked.
pub fn compact<'a>(f: Field, selected: Option<&'a Token>, fallback: &'a str, purpose: Purpose) -> El<'a> {
    let symbol = selected.map(|t| t.symbol.clone()).unwrap_or_else(|| fallback.to_string());
    let shown = selected.cloned().unwrap_or(Token { symbol: symbol.clone(), name: String::new(), address: String::new(), decimals: 18, logo: None });
    button(row![token_picker::logo(&shown, f.logo.as_ref(), 20.0), text(symbol).size(13).font(BOLD).color(Color::WHITE), icon(icons::chevron_down(), 12, MUTED)].spacing(6).align_y(Alignment::Center))
        .padding([5, 10])
        .style(field_style)
        .on_press(open(purpose))
        .into()
}
