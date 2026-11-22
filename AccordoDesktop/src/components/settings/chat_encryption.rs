// SPDX-License-Identifier: GPL-3.0-or-later
//! "Chat encryption": what E2EE is, the chat-password state and the key actions.

use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Color, Fill};

use super::{action, card, note, pill};
use crate::api::auth::User;
use crate::components::file_picker::{PickerMsg, Purpose};
use crate::components::key_backup_modal::Mode;
use crate::e2ee::account::ChatKey;
use crate::app::Message;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::settings_page::SettingsMsg;
use crate::theme::*;

pub fn view<'a>(user: &'a User, how_it_works: bool, key: &'a ChatKey) -> El<'a> {
    let status = match key {
        ChatKey::Ready(_) => pill("● ON THIS COMPUTER", GREEN, rgba(0x10b981, 0.12), rgba(0x10b981, 0.3)),
        ChatKey::Loading => pill("…", TEXT_SECONDARY, rgba(0xffffff, 0.06), rgba(0xffffff, 0.0)),
        _ if user.e2ee_public_key.is_none() => pill("NOT SET UP", AMBER_TEXT, rgba(0xf59e0b, 0.12), rgba(0xf59e0b, 0.3)),
        _ => pill("NOT ON THIS COMPUTER", AMBER_TEXT, rgba(0xf59e0b, 0.12), rgba(0xf59e0b, 0.3)),
    };

    let mut body = column![
        note("Private chats and groups are encrypted on your device with your own chat key. Accordo only ever stores encrypted messages — and, if you set a chat password, an encrypted backup of your key that only you can open."),
        button(
            row![icon(if how_it_works { icons::chevron_down() } else { icons::chevron_right() }, 12, TEXT_SECONDARY), text("How it works").size(12).color(TEXT_SECONDARY)]
                .spacing(4)
                .align_y(Alignment::Center)
        )
        .padding(0)
        .style(link_button)
        .on_press(Message::Settings(SettingsMsg::ToggleHowItWorks)),
    ]
    .spacing(12);

    if how_it_works {
        body = body.push(
            container(note("Each account has a key pair. Messages are encrypted for the people in the chat before they leave your device; the server relays and stores them without being able to read them. Your private key never leaves the device unencrypted: the backup is sealed with your chat password."))
                .padding([8, 12])
                .style(crate::theme::soft_row),
        );
    }

    // What this computer has decides what can be done here.
    let open = |mode: Mode| Message::Settings(SettingsMsg::OpenKeyBackup(mode));
    let (state_label, state_color, explanation, button): (&str, Color, String, Option<(&str, Message)>) = match key {
        ChatKey::Loading => ("…", TEXT_SECONDARY, "Looking for your chat key on this computer…".into(), None),
        ChatKey::NoStore(e) => (
            "Unavailable",
            DANGER_TEXT,
            format!("The system's secret store can't be used ({e}), so the chat key can't be kept on this computer."),
            None,
        ),
        ChatKey::Ready(_) if user.has_key_backup => (
            "Backed up",
            GREEN,
            "Your chat key is on this computer and has an encrypted backup on Accordo, sealed with your chat password.".into(),
            Some(("Change chat password", open(Mode::Change))),
        ),
        ChatKey::Ready(_) => (
            "Not set",
            AMBER_TEXT,
            "Your chat key has no backup yet — losing every device that holds it would lose your encrypted chats. Set a chat password (separate from your wallet password).".into(),
            Some(("Set chat password", open(Mode::Create))),
        ),
        _ if user.has_key_backup => (
            "Backed up",
            GREEN,
            "Your key has an encrypted backup on Accordo. This computer doesn't have it yet: restore it with your chat password to read encrypted chats here.".into(),
            Some(("Restore with chat password", open(Mode::Restore))),
        ),
        ChatKey::Different(_) => (
            "Different key",
            AMBER_TEXT,
            "This computer holds a different chat key than your account uses now. Load the current one from a key file, or set a chat password where your chats open and restore it here.".into(),
            None,
        ),
        _ if user.e2ee_public_key.is_some() => (
            "No backup",
            AMBER_TEXT,
            "Your chat key is only on the device where you chat now. There, set a chat password (Settings → Chat encryption) or download a key file — then bring it here.".into(),
            None,
        ),
        _ => ("Not set up", AMBER_TEXT, "Your account has no chat key yet.".into(), None),
    };
    let good = state_color == GREEN;
    let mut password_box = column![
        row![text("Chat password").size(13).font(BOLD).color(Color::WHITE), space().width(Fill), text(state_label).size(11.5).font(BOLD).color(state_color)].align_y(Alignment::Center),
        text(explanation).size(12).color(TEXT_SECONDARY).line_height(1.5),
    ]
    .spacing(10);
    if let Some((label, message)) = button {
        password_box = password_box.push(action(label, None, Tone::Accent, message, false));
    }
    body = body.push(container(password_box).width(Fill).padding([12, 14]).style(if good { crate::theme::soft_row } else { notice }));

    body = body
        .push(
            row![
                action("Download key file", Some(icons::download()), Tone::Accent, Message::NotYet("Downloading the key file"), true),
                action("Load key file", Some(icons::upload()), Tone::Dark, Message::Picker(PickerMsg::Open(Purpose::ChatKeyFile)), true),
            ]
            .spacing(8),
        )
        .push(action("Reset contacts' trusted keys", None, Tone::Dark, Message::NotYet("Trusted keys"), true))
        .push(action("Create a new chat key", None, Tone::Danger, Message::NotYet("Creating a chat key"), true));

    card(icons::key_round(), 0x818cf8, "Chat encryption", Some(status), true, body.into())
}
