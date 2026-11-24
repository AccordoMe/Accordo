// SPDX-License-Identifier: GPL-3.0-or-later
//! The chat password (KeyBackupModal.tsx): set it (seals a backup of the chat key on Accordo),
//! change it, or restore the key from that backup on this computer. The work itself is done by
//! the app (e2ee::account); this is the form.

use iced::alignment::Horizontal;
use iced::widget::{button, center, checkbox, column, container, row, space, text, text_input, Column};
use iced::{Alignment, Color, Fill, Font, Padding};

use super::overlay;
use super::ui::{icon, El};
use crate::app::Message;
use crate::e2ee::phrase::{generate_phrase, phrase_problem};
use crate::icons;
use crate::theme::*;

const MIN_LEN: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Create,
    Change,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Password,
    SeedPhrase,
}

#[derive(Debug, Clone)]
pub enum KeyBackupMsg {
    Tab(Tab),
    Password(String),
    Repeat(String),
    ToggleShow,
    Generate,
    NewPhrase,
    PhraseSaved(bool),
    Submit,
    /// The work finished: Ok(what to tell the user) or Err(why, shown in the form).
    Done(Result<String, String>),
    Close,
}

#[derive(Default)]
pub struct State {
    pub mode: Mode,
    pub tab: Tab,
    pub password: String,
    pub repeat: String,
    pub show: bool,
    pub phrase: String,
    pub phrase_saved: bool,
    pub busy: bool,
    pub error: Option<String>,
    /// Opened unasked when the account opens (the web's ChatPasswordReminder): the risk note and
    /// a "Not now".
    pub reminder: bool,
}

impl State {
    pub fn new(mode: Mode) -> Self {
        Self { mode, ..Default::default() }
    }

    /// The reminder: restore the key from the backup, or set a chat password to make one.
    pub fn reminder(mode: Mode) -> Self {
        Self { mode, reminder: true, ..Default::default() }
    }

    /// The secret to use, once the form is complete.
    pub fn secret(&self) -> Option<String> {
        match (self.mode, self.tab) {
            (Mode::Restore, _) => (!self.password.trim().is_empty()).then(|| self.password.clone()),
            (_, Tab::Password) => (self.password.chars().count() >= MIN_LEN && self.password == self.repeat).then(|| self.password.clone()),
            (_, Tab::SeedPhrase) => (self.phrase_saved && phrase_problem(&self.phrase).is_none()).then(|| self.phrase.clone()),
        }
    }

    pub fn generate(&mut self) {
        if let Some(password) = random_password() {
            self.repeat = password.clone();
            self.password = password;
            self.show = true;
        }
    }

    pub fn update(&mut self, msg: KeyBackupMsg) {
        match msg {
            KeyBackupMsg::Tab(tab) => {
                self.tab = tab;
                if tab == Tab::SeedPhrase && self.phrase.is_empty() {
                    self.phrase = generate_phrase();
                }
            }
            KeyBackupMsg::Password(v) => self.password = v,
            KeyBackupMsg::Repeat(v) => self.repeat = v,
            KeyBackupMsg::ToggleShow => self.show = !self.show,
            KeyBackupMsg::Generate => self.generate(),
            KeyBackupMsg::NewPhrase => {
                self.phrase = generate_phrase();
                self.phrase_saved = false;
            }
            KeyBackupMsg::PhraseSaved(v) => self.phrase_saved = v,
            KeyBackupMsg::Done(Err(e)) => {
                self.busy = false;
                self.error = Some(e);
            }
            KeyBackupMsg::Submit | KeyBackupMsg::Done(Ok(_)) | KeyBackupMsg::Close => {}
        }
    }
}

/// A random 20-character password (letters and digits without look-alikes), shown so it can be
/// copied. Bytes past the largest multiple of the alphabet size are skipped, so every character
/// is equally likely.
pub fn random_password() -> Option<String> {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let limit = 256 - 256 % ALPHABET.len();
    let mut out = String::with_capacity(20);
    while out.len() < 20 {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).ok()?;
        out.extend(bytes.iter().filter(|b| (**b as usize) < limit).map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char));
    }
    out.truncate(20);
    Some(out)
}

fn copy(mode: Mode) -> (&'static str, &'static str, &'static str, &'static str) {
    match mode {
        Mode::Create => (
            "Set your chat password",
            "Your chat password protects your chat key — on this computer and in an encrypted backup on Accordo that brings your chats back on a new device. It's separate from your wallet password, and you don't need it to chat.",
            "Set chat password",
            "Encrypting…",
        ),
        Mode::Change => (
            "Change your chat password",
            "Choose a new chat password. The backup on Accordo is re-encrypted with it; the old password stops opening it.",
            "Change password",
            "Re-encrypting…",
        ),
        Mode::Restore => (
            "Restore your chat key",
            "Enter your chat password to bring your chat key back from the encrypted backup. It stays in this computer's secret store.",
            "Restore",
            "Opening…",
        ),
    }
}

pub fn view(state: &State) -> El<'_> {
    let msg = Message::KeyBackup;
    let (title, intro, submit_label, busy_label) = copy(state.mode);
    let header = row![
        icon(icons::key_round(), 17, INDIGO_SOFT),
        text(title).size(16).font(BOLD).color(Color::WHITE),
        space().width(Fill),
        button(icon(icons::x(), 16, MUTED)).padding(2).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(msg(KeyBackupMsg::Close)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut form: Column<'_, Message> = column![header, text(intro).size(12.5).color(TEXT_SECONDARY).line_height(1.5)].spacing(14);
    if state.reminder {
        let risk = if state.mode == Mode::Restore {
            "Your chat key is backed up on Accordo, but this computer doesn't have it yet. Restore it with your chat password to open your encrypted chats here — don't set a new password, that would start a new key."
        } else {
            "Right now your chat key exists only on this computer. Lose it, or reinstall the system, and your encrypted chats are gone for good."
        };
        form = form.push(text(risk).size(12.5).color(AMBER_TEXT).line_height(1.5));
    }

    let secret_input = |placeholder: &str, value: &str, on: fn(String) -> KeyBackupMsg| {
        text_input(placeholder, value).secure(!state.show).size(14).padding([10, 12]).style(field).on_input(move |v| Message::KeyBackup(on(v))).on_submit(msg(KeyBackupMsg::Submit))
    };

    if state.mode == Mode::Restore {
        form = form.push(
            text_input("Chat password or 12-word phrase", &state.password)
                .secure(true)
                .size(14)
                .padding([10, 12])
                .style(field)
                .on_input(|v| Message::KeyBackup(KeyBackupMsg::Password(v)))
                .on_submit(msg(KeyBackupMsg::Submit)),
        );
    } else {
        form = form.push(
            container(
                row![
                    button(center(text("Password").size(12).font(BOLD)).height(18)).width(Fill).padding([7, 0]).style(segment(state.tab == Tab::Password)).on_press(msg(KeyBackupMsg::Tab(Tab::Password))),
                    button(center(text("Seed phrase (12 words)").size(12).font(BOLD)).height(18)).width(Fill).padding([7, 0]).style(segment(state.tab == Tab::SeedPhrase)).on_press(msg(KeyBackupMsg::Tab(Tab::SeedPhrase))),
                ]
                .spacing(4),
            )
            .padding(3)
            .style(boxed(rgba(0xffffff, 0.03), BORDER, 9.0)),
        );
        match state.tab {
            Tab::Password => {
                let eye = button(icon(if state.show { icons::eye_off() } else { icons::eye() }, 15, MUTED)).padding([0, 4]).style(ghost(4.0, Color::TRANSPARENT)).on_press(msg(KeyBackupMsg::ToggleShow));
                let mismatch = !state.repeat.is_empty() && state.password != state.repeat;
                form = form
                    .push(
                        container(row![secret_input("Chat password", &state.password, KeyBackupMsg::Password).padding([10, 0]).style(bare_input), eye].align_y(Alignment::Center))
                            .padding(Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 12.0 })
                            .style(boxed(hex(0x0e0f17), BORDER, 8.0)),
                    )
                    .push(secret_input("Repeat password", &state.repeat, KeyBackupMsg::Repeat))
                    .push(
                        row![
                            if mismatch { text("Passwords don't match").size(11.5).color(DANGER_TEXT) } else { text(format!("At least {MIN_LEN} characters")).size(11.5).color(TEXT_SECONDARY) },
                            space().width(Fill),
                            button(row![icon(icons::wand_sparkles(), 12, hex(0xc7d2fe)), text("Generate password").size(11.5).font(BOLD)].spacing(5).align_y(Alignment::Center))
                                .padding([4, 9])
                                .style(outline_indigo)
                                .on_press(msg(KeyBackupMsg::Generate)),
                        ]
                        .align_y(Alignment::Center),
                    );
            }
            Tab::SeedPhrase => {
                let mut words = Column::new().spacing(6);
                let list: Vec<&str> = state.phrase.split(' ').collect();
                for (r, chunk) in list.chunks(3).enumerate() {
                    let mut line = row![].spacing(6);
                    for (c, w) in chunk.iter().enumerate() {
                        line = line.push(
                            container(row![text(format!("{:>2}", r * 3 + c + 1)).size(11).font(Font::MONOSPACE).color(FAINT), text(w.to_string()).size(13).font(Font::MONOSPACE).color(Color::WHITE)].spacing(8))
                                .width(Fill)
                                .padding([6, 10])
                                .style(boxed(hex(0x0e0f17), BORDER, 6.0)),
                        );
                    }
                    words = words.push(line);
                }
                form = form.push(words).push(
                    row![
                        checkbox(state.phrase_saved).label("I've written these 12 words down").size(14).text_size(12.5).style(tick).on_toggle(|v| Message::KeyBackup(KeyBackupMsg::PhraseSaved(v))),
                        space().width(Fill),
                        button(text("New phrase").size(11.5)).padding([4, 9]).style(outline_indigo).on_press(msg(KeyBackupMsg::NewPhrase)),
                    ]
                    .align_y(Alignment::Center),
                );
            }
        }
        form = form.push(
            container(text("Forget it and nobody can restore your chats on a new device — not even Accordo. Keep it different from your login and wallet passwords.").size(12).color(AMBER_TEXT).line_height(1.5))
                .width(Fill)
                .padding([9, 11])
                .style(notice),
        );
    }

    if let Some(e) = &state.error {
        form = form.push(text(e.clone()).size(12.5).color(DANGER_TEXT).line_height(1.45));
    }
    let ready = state.secret().is_some() && !state.busy;
    form = form.push(
        button(center(text(if state.busy { busy_label } else { submit_label }).size(14).font(MEDIUM)).height(20))
            .width(Fill)
            .padding([9, 0])
            .style(tone_button(Tone::Accent))
            .on_press_maybe(ready.then(|| msg(KeyBackupMsg::Submit))),
    );
    if state.reminder {
        form = form.push(
            container(button(text("Not now — remind me next time").size(12.5).color(MUTED)).padding([2, 6]).style(ghost(6.0, rgba(0xffffff, 0.04))).on_press(msg(KeyBackupMsg::Close)))
                .width(Fill)
                .align_x(Horizontal::Center),
        );
    }

    overlay::modal(container(form.align_x(Horizontal::Left)).width(460).padding(20).style(modal), msg(KeyBackupMsg::Close))
}
