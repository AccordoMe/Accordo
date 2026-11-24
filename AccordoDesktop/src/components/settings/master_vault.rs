// SPDX-License-Identifier: GPL-3.0-or-later
//! "Master Password & Vault" (SettingsPage.tsx, SecretField.tsx, WalletLockControls.tsx): the
//! password that seals wallet keys on this device. Three states, as on the web: choose one (no
//! password yet), unlock (wallets locked), unlocked (countdown, Lock now). The work itself — key
//! derivation, re-sealing, syncing — is done by the app (e2ee::wallet_session).

use iced::widget::{button, center, checkbox, column, container, row, space, text, text_input, Column, Row};
use iced::{Alignment, Color, Fill, Font, Padding};

use super::{card, note};
use crate::app::Message;
use crate::components::key_backup_modal::Tab;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::settings_page::{SettingsMsg, VaultForm, LOCK_OPTIONS, VAULT_MIN_LEN};
use crate::theme::*;

/// The session as the card shows it.
#[derive(Debug, Clone, Copy)]
pub struct Status {
    pub unlocked: bool,
    /// When it locks (ms since the epoch); None: until restart.
    pub lock_at: Option<i64>,
    pub now_ms: i64,
    /// A password exists: the check vault on this computer, or wallets sealed with one.
    pub has_password: bool,
    /// Wallets on the account sealed with it (forgetting it loses them).
    pub has_sealed: bool,
}

fn msg(m: SettingsMsg) -> Message {
    Message::Settings(m)
}

fn left(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// "🔓 Unlocked · locks in 42:10 · Lock now" (WalletLockStatus).
fn lock_status<'a>(st: Status) -> El<'a> {
    let green = hex(0x34d399);
    let when = match st.lock_at {
        Some(t) => format!("· locks in {}", left(t - st.now_ms)),
        None => "· until the app is closed".into(),
    };
    container(
        row![
            icon(icons::lock_open(), 13, green),
            text("Unlocked").size(12).font(SEMIBOLD).color(green),
            text(when).size(12).color(TEXT_SECONDARY),
            space().width(Fill),
            button(row![icon(icons::lock(), 11, green), text("Lock now").size(11.5).font(SEMIBOLD).color(green)].spacing(4).align_y(Alignment::Center))
                .padding([3, 8])
                .style(move |_, status| button::Style {
                    background: matches!(status, button::Status::Hovered).then(|| rgba(0x10b981, 0.1).into()),
                    border: border(rgba(0x10b981, 0.35), 1.0, 6.0),
                    ..Default::default()
                })
                .on_press(msg(SettingsMsg::VaultLockNow)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([8, 12])
    .style(boxed(rgba(0x10b981, 0.06), rgba(0x10b981, 0.2), 8.0))
    .into()
}

fn timers<'a>(form: &VaultForm) -> El<'a> {
    let mut chips: Row<'_, Message> = row![].spacing(6);
    for (i, (caption, _)) in LOCK_OPTIONS.iter().enumerate() {
        chips = chips.push(button(text(*caption).size(11)).padding([3, 8]).style(chip(form.lock == i)).on_press(msg(SettingsMsg::VaultLock(i))));
    }
    let hint = if LOCK_OPTIONS[form.lock].1 == 0 {
        "Locks again whenever the app is closed. The keys stay in this app's memory only."
    } else {
        "Locks again then. Until then the unlocked keys are kept in the system's secret store, so a restart doesn't lock them (App settings → Storage deletes them). Your password is never stored."
    };
    column![text("Keep unlocked").size(12).font(SEMIBOLD).color(LABEL), chips, note(hint)].spacing(6).into()
}

fn small_button<'a>(handle: iced::widget::svg::Handle, caption: &'a str, on: SettingsMsg) -> El<'a> {
    button(row![icon(handle, 11, hex(0xc7d2fe)), text(caption).size(11).font(BOLD)].spacing(4).align_y(Alignment::Center))
        .padding([3, 8])
        .style(outline_indigo)
        .on_press(msg(on))
        .into()
}

/// Choosing a new secret (SecretField): password typed twice, or 12 words; or generated.
fn secret_field<'a>(form: &'a VaultForm) -> El<'a> {
    let tabs = container(
        row![
            button(center(text("Password").size(11.5).font(BOLD)).height(16)).width(Fill).padding([6, 0]).style(segment(form.tab == Tab::Password)).on_press(msg(SettingsMsg::VaultTab(Tab::Password))),
            button(center(text("Seed phrase (12 words)").size(11.5).font(BOLD)).height(16)).width(Fill).padding([6, 0]).style(segment(form.tab == Tab::SeedPhrase)).on_press(msg(SettingsMsg::VaultTab(Tab::SeedPhrase))),
        ]
        .spacing(4),
    )
    .padding(3)
    .style(boxed(rgba(0xffffff, 0.03), BORDER, 9.0));

    let mut col: Column<'_, Message> = column![tabs].spacing(8);
    match form.tab {
        Tab::Password => {
            // Hidden unless the eye is pressed — a generated one too (Copy takes it out unseen).
            let eye = button(icon(if form.show { icons::eye_off() } else { icons::eye() }, 14, MUTED)).padding([0, 4]).style(ghost(4.0, Color::TRANSPARENT)).on_press(msg(SettingsMsg::VaultShow));
            let mut input = text_input("Master password", &form.value).secure(!form.show).size(13).padding([9, 0]).style(bare_input).on_input(|v| msg(SettingsMsg::VaultValue(v))).on_submit(msg(SettingsMsg::VaultSave));
            if form.show {
                input = input.font(Font::MONOSPACE);
            }
            col = col.push(
                container(row![input, eye].align_y(Alignment::Center))
                    .padding(Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 12.0 })
                    .style(boxed(hex(0x0e0f17), BORDER, 8.0)),
            );
            if !form.generated {
                col = col.push(
                    text_input("Repeat password", &form.confirm).secure(true).size(13).padding([9, 12]).style(field).on_input(|v| msg(SettingsMsg::VaultConfirm(v))).on_submit(msg(SettingsMsg::VaultSave)),
                );
            }
        }
        Tab::SeedPhrase if form.generated => {
            let words: Vec<&str> = form.value.split(' ').collect();
            let mut grid = Column::new().spacing(6);
            for (r, chunk) in words.chunks(3).enumerate() {
                let mut line = row![].spacing(6);
                for (c, w) in chunk.iter().enumerate() {
                    line = line.push(
                        container(row![text(format!("{:>2}.", r * 3 + c + 1)).size(11).font(Font::MONOSPACE).color(FAINT), text(w.to_string()).size(12.5).font(Font::MONOSPACE).color(TEXT_BODY)].spacing(6))
                            .width(Fill)
                            .padding([5, 8]),
                    );
                }
                grid = grid.push(line);
            }
            col = col.push(container(grid).width(Fill).padding(6).style(boxed(hex(0x0e0f17), BORDER, 8.0)));
        }
        Tab::SeedPhrase => {
            col = col.push(
                text_input("Type or paste 12 words separated by spaces", &form.value)
                    .font(Font::MONOSPACE)
                    .size(13)
                    .padding([9, 12])
                    .style(field)
                    .on_input(|v| msg(SettingsMsg::VaultValue(v)))
                    .on_submit(msg(SettingsMsg::VaultSave)),
            );
        }
    }

    // Status line (SecretField): problem in red, done in green, "save it" in amber.
    let problem = form.problem();
    let complete = form.secret().is_some();
    let (status, colour) = match (&problem, complete, form.generated, form.tab) {
        (Some(p), ..) => (p.clone(), hex(0xf87171)),
        (None, true, _, Tab::SeedPhrase) => ("12 words ✓".into(), hex(0x34d399)),
        (None, true, _, Tab::Password) => ("Strong enough ✓".into(), hex(0x34d399)),
        (None, false, true, _) => ("Save it, then tick the box below".into(), hex(0xfbbf24)),
        (None, false, false, Tab::SeedPhrase) => ("Use the 12 words in this order".into(), TEXT_SECONDARY),
        (None, false, false, Tab::Password) => (format!("At least {VAULT_MIN_LEN} characters"), TEXT_SECONDARY),
    };
    let generate = match (form.generated, form.tab) {
        (true, _) => "Generate another",
        (false, Tab::SeedPhrase) => "Generate phrase",
        (false, Tab::Password) => "Generate password",
    };
    let mut actions = row![].spacing(6);
    if form.generated {
        actions = actions.push(small_button(icons::copy(), "Copy", SettingsMsg::VaultCopy));
    }
    actions = actions.push(small_button(icons::wand_sparkles(), generate, SettingsMsg::VaultGenerate));
    col = col.push(row![text(status).size(11).color(colour), space().width(Fill), actions].spacing(8).align_y(Alignment::Center));

    if form.generated {
        col = col.push(
            checkbox(form.saved)
                .label("I've written it down or saved it in a password manager. Without it this can't be recovered.")
                .size(14)
                .text_size(12)
                .style(tick)
                .on_toggle(|v| msg(SettingsMsg::VaultSavedIt(v))),
        );
    }
    col.into()
}

/// "Forgot master password?": nobody can recover it, so the way out is starting over.
fn reset_panel<'a>(form: &'a VaultForm, st: Status) -> El<'a> {
    let danger_box = |body: El<'a>| -> El<'a> { container(body).width(Fill).padding(12).style(boxed(rgba(0xef4444, 0.06), rgba(0xef4444, 0.35), 10.0)).into() };
    let cancel = button(text("Cancel").size(12)).padding([7, 12]).style(tone_button(Tone::Dark)).on_press(msg(SettingsMsg::VaultForgot(false)));
    if !st.has_sealed {
        return danger_box(
            column![
                note("No wallets are stored with this master password, so nothing is lost — resetting just lets you choose a new one."),
                row![
                    button(center(text(if form.resetting { "Resetting…" } else { "Reset master password" }).size(12)).height(16))
                        .width(Fill)
                        .padding([7, 12])
                        .style(tone_button(Tone::Danger))
                        .on_press_maybe((!form.resetting).then(|| msg(SettingsMsg::VaultReset))),
                    cancel,
                ]
                .spacing(8),
            ]
            .spacing(8)
            .into(),
        );
    }
    let typed = form.reset_confirm.trim() == "RESET";
    danger_box(
        column![
            text("Nobody can recover a master password — not even Accordo. You can remove these wallets from your account and start over, but their private keys are lost for good: anything still on them stays there unless you saved the keys somewhere else.")
                .size(12)
                .color(hex(0xfca5a5))
                .line_height(1.5),
            text_input("Type RESET to confirm", &form.reset_confirm).size(12.5).padding([8, 10]).style(field).on_input(|v| msg(SettingsMsg::VaultResetConfirm(v))),
            row![
                button(center(text(if form.resetting { "Removing…" } else { "Remove wallets and reset" }).size(12)).height(16))
                    .width(Fill)
                    .padding([7, 12])
                    .style(tone_button(Tone::Danger))
                    .on_press_maybe((typed && !form.resetting).then(|| msg(SettingsMsg::VaultReset))),
                cancel,
            ]
            .spacing(8),
        ]
        .spacing(8)
        .into(),
    )
}

pub fn view(form: &VaultForm, st: Status) -> El<'_> {
    let body: El = if st.unlocked {
        column![
            note("Your wallets are unlocked in this app. Their private keys are encrypted with your master password, and only an encrypted copy is kept with your account — Accordo can't open it."),
            lock_status(st),
            space().height(Fill),
            button(center(text("Lock wallets now").size(12).font(SEMIBOLD)).height(16)).width(Fill).padding([9, 12]).style(tone_button(Tone::Danger)).on_press(msg(SettingsMsg::VaultLockNow)),
        ]
        .spacing(12)
        .into()
    } else {
        let (intro, input, ready, label): (&str, El, bool, &str) = if st.has_password {
            (
                "Your wallets are locked. Enter your master password (or 12-word phrase) to unlock them in this app.",
                text_input("Master password or 12-word phrase", &form.unlock)
                    .secure(true)
                    .size(13)
                    .padding([9, 12])
                    .style(field)
                    .on_input(|v| msg(SettingsMsg::VaultUnlockInput(v)))
                    .on_submit(msg(SettingsMsg::VaultSave))
                    .into(),
                !form.unlock.trim().is_empty(),
                "Unlock wallets",
            )
        } else {
            (
                "Set a master password to encrypt your wallet keys on this device. It never leaves your device, and it's separate from your chat password.",
                secret_field(form),
                form.secret().is_some(),
                "Save Master Password",
            )
        };
        let mut col: Column<'_, Message> = column![
            note(intro),
            input,
            timers(form),
            button(center(text(if form.busy { "Unlocking..." } else { label }).size(13).font(SEMIBOLD)).height(18))
                .width(Fill)
                .padding([8, 0])
                .style(tone_button(Tone::Accent))
                .on_press_maybe((ready && !form.busy).then(|| msg(SettingsMsg::VaultSave))),
        ]
        .spacing(12);
        if st.has_password {
            col = col.push(if form.reset_open {
                reset_panel(form, st)
            } else {
                container(button(text("Forgot master password?").size(12).color(TEXT_SECONDARY)).padding(0).style(link_button).on_press(msg(SettingsMsg::VaultForgot(true)))).width(Fill).center_x(Fill).into()
            });
        }
        col.into()
    };
    card(icons::lock(), 0x10b981, "Master Password & Vault", None, false, body)
}

/// The wallet drawer's unlock form (WalletModal): same password field, timer and button.
pub fn unlock_form(form: &VaultForm) -> El<'_> {
    column![
        text("Enter your master password to unlock your wallets.").size(13).color(TEXT_SECONDARY),
        text_input("Master password or 12-word phrase", &form.unlock)
            .secure(true)
            .size(13)
            .padding(12)
            .style(field)
            .on_input(|v| msg(SettingsMsg::VaultUnlockInput(v)))
            .on_submit(msg(SettingsMsg::VaultSave)),
        timers(form),
        button(center(text(if form.busy { "Unlocking..." } else { "Unlock Vault" }).size(13).font(SEMIBOLD)).height(18))
            .width(Fill)
            .padding([9, 0])
            .style(tone_button(Tone::Accent))
            .on_press_maybe((!form.busy && !form.unlock.trim().is_empty()).then(|| msg(SettingsMsg::VaultSave))),
    ]
    .spacing(12)
    .into()
}

/// "🔓 Unlocked · locks in …" for the wallet drawer.
pub fn lock_badge<'a>(st: Status) -> El<'a> {
    lock_status(st)
}
