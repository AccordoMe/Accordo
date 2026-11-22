// SPDX-License-Identifier: GPL-3.0-or-later
//! The window before the Accordo wallet signs (the web's openDetailsWindow + useTxStage): what is
//! being created, from which wallet, on which contracts — then the same window shows the steps
//! (permission, signature, block, saving to Accordo) and how it ended.
//!
//! The Accordo wallet signs with no wallet app of its own, so this is the one place the person
//! sees what they sign. Nothing is signed until "Confirm" is pressed.

use alloy_primitives::B256;
use iced::widget::{button, column, container, row, scrollable, space, text, Column};
use iced::{Alignment, Color, Fill, Length, Padding};

use crate::app::Message;
use crate::components::overlay;
use crate::components::ui::{icon, El};
use crate::evm::client::Stage;
use crate::icons;
use crate::theme::*;

/// One line of the details.
#[derive(Debug, Clone)]
pub enum Item {
    Row { label: String, value: String, strong: bool, mono: bool },
    /// An address or contract: shown short ("0x3Dd0…7590"), whole on hover, with a copy button.
    Address { label: String, address: String },
    Section(&'static str),
    Note(String),
}

impl Item {
    pub fn row(label: impl Into<String>, value: impl Into<String>) -> Self {
        Item::Row { label: label.into(), value: value.into(), strong: false, mono: false }
    }
    pub fn strong(label: impl Into<String>, value: impl Into<String>) -> Self {
        Item::Row { label: label.into(), value: value.into(), strong: true, mono: false }
    }
    pub fn note(t: impl Into<String>) -> Self {
        Item::Note(t.into())
    }
    pub fn address(label: impl Into<String>, address: impl Into<String>) -> Self {
        Item::Address { label: label.into(), address: address.into() }
    }
}

/// What is shown before signing.
#[derive(Debug, Clone)]
pub struct Review {
    pub title: String,
    pub subtitle: String,
    /// "Create", "Pay"…
    pub confirm: &'static str,
    pub items: Vec<Item>,
    /// A token: an approve may come first.
    pub approve: bool,
    /// The record is saved to Accordo after the transaction.
    pub record: bool,
    /// "Cheque created".
    pub done_title: &'static str,
    /// Accordo submits it and pays the gas (AccordoTask claims): nothing is signed here.
    pub gasless: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Review,
    Approving,
    Signing,
    Confirming,
    Saving,
    Done,
}

/// How it ended, when it went through.
#[derive(Debug, Clone)]
pub struct Finished {
    pub hash: B256,
    /// Saved to Accordo; false: kept on this computer (store/unsaved.rs).
    pub saved: bool,
    /// The backend's answer had a problem worth showing (not an error of the transaction).
    pub warning: Option<String>,
    /// What it made, when that's new (a cheque's id) — for a module that asked for it.
    pub id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum TxMsg {
    Confirm,
    Close,
    Stage(Stage, Option<B256>),
    Saving(B256),
    Done(Result<Finished, Failed>),
    CopyHash,
    OpenTx,
    /// Copy a whole address from the details.
    Copy(String),
}

/// A failure, with the hash when the transaction was already sent.
#[derive(Debug, Clone)]
pub struct Failed {
    pub message: String,
    pub hash: Option<B256>,
}

pub struct State<J> {
    pub review: Review,
    pub step: Step,
    pub hash: Option<B256>,
    pub error: Option<String>,
    pub finished: Option<Finished>,
    /// "{explorer}/tx/{hash}" base, when the network has one.
    pub explorer: Option<String>,
    /// What to run on Confirm (taken when it starts).
    pub job: Option<J>,
}

impl<J> State<J> {
    pub fn new(review: Review, explorer: Option<String>, job: J) -> Self {
        Self { review, step: Step::Review, hash: None, error: None, finished: None, explorer, job: Some(job) }
    }

    /// Signing or waiting: the window can't be closed (the transaction would go on unseen).
    pub fn running(&self) -> bool {
        self.error.is_none() && self.step > Step::Review && self.step < Step::Done
    }

    pub fn tx_url(&self) -> Option<String> {
        Some(format!("{}/tx/{:#x}", self.explorer.as_deref()?.trim_end_matches('/'), self.hash?))
    }

    /// A progress message; ignored once it's over (they can arrive after Done).
    pub fn apply(&mut self, m: &TxMsg) {
        if self.error.is_some() || self.step == Step::Done {
            return;
        }
        match m {
            TxMsg::Stage(s, h) => {
                let step = match s {
                    Stage::Approving => Step::Approving,
                    Stage::Signing => Step::Signing,
                    Stage::Confirming => Step::Confirming,
                };
                if step >= self.step {
                    self.step = step;
                }
                if h.is_some() && step != Step::Approving {
                    self.hash = *h;
                }
            }
            TxMsg::Saving(h) => {
                self.hash = Some(*h);
                self.step = Step::Saving;
            }
            TxMsg::Done(Ok(f)) => {
                self.hash = Some(f.hash);
                self.finished = Some(f.clone());
                self.step = Step::Done;
            }
            TxMsg::Done(Err(e)) => {
                if e.hash.is_some() {
                    self.hash = e.hash;
                }
                self.error = Some(e.message.clone());
            }
            _ => {}
        }
    }
}

fn msg(m: TxMsg) -> Message {
    Message::Tx(m)
}

/// The steps on the left, ticked off as they go.
fn steps<'a, J>(st: &State<J>) -> El<'a> {
    let mut list: Vec<(Step, &'static str, &'static str)> = vec![(Step::Review, "Review", "Check what's below")];
    if st.review.gasless {
        list.push((Step::Signing, "Claim", "Accordo submits it and pays the gas"));
    } else if st.review.approve {
        list.push((Step::Approving, "Permission", "Only if the token needs it"));
    }
    if !st.review.gasless {
        list.push((Step::Signing, "Sign", "With your Accordo wallet"));
        list.push((Step::Confirming, "On-chain", "Waiting for the block"));
    }
    if st.review.record {
        list.push((Step::Saving, "Save to Accordo", "So it shows up for them"));
    }
    let mut col: Column<'a, Message> = column![].spacing(4);
    for (step, title, hint) in list {
        let done = st.step > step || (st.step == Step::Done);
        let current = st.step == step && !done;
        let failed = current && st.error.is_some();
        let (mark, color): (El, Color) = if failed {
            (icon(icons::x(), 12, DANGER_TEXT).into(), DANGER_TEXT)
        } else if done {
            (icon(icons::check(), 12, GREEN).into(), TEXT_BODY)
        } else if current {
            (container(space()).width(8).height(8).style(boxed(INDIGO, Color::TRANSPARENT, 4.0)).into(), Color::WHITE)
        } else {
            (container(space()).width(8).height(8).style(boxed(rgba(0xffffff, 0.12), Color::TRANSPARENT, 4.0)).into(), FAINT)
        };
        let dot = container(mark).width(22).height(22).center(22).style(boxed(if current { rgba(0x6366f1, 0.16) } else { rgba(0xffffff, 0.04) }, if current { rgba(0x6366f1, 0.45) } else { BORDER }, 11.0));
        col = col.push(
            container(row![dot, column![text(title).size(13).font(if current { BOLD } else { SEMIBOLD }).color(color), text(hint).size(11).color(FAINT)].spacing(1)].spacing(10).align_y(Alignment::Center))
                .width(Fill)
                .padding([8, 10])
                .style(if current { boxed(rgba(0xffffff, 0.04), Color::TRANSPARENT, 8.0) } else { boxed(Color::TRANSPARENT, Color::TRANSPARENT, 8.0) }),
        );
    }
    col.into()
}

fn items<'a>(list: &'a [Item]) -> El<'a> {
    let mut col: Column<'a, Message> = column![].spacing(2);
    for it in list {
        let line: El = match it {
            Item::Section(t) => container(text(t.to_uppercase()).size(10).font(BOLD).color(FAINT)).padding(Padding { top: 14.0, right: 0.0, bottom: 4.0, left: 0.0 }).into(),
            Item::Note(t) => container(row![icon(icons::info(), 13, INDIGO_SOFT), text(t.as_str()).size(12).color(TEXT_SECONDARY)].spacing(8))
                .width(Fill)
                .padding([8, 12])
                .style(boxed(rgba(0x6366f1, 0.06), rgba(0x6366f1, 0.2), 8.0))
                .into(),
            Item::Address { label, address } => {
                let shown = text(crate::wallets::short_address(address)).size(13).font(iced::Font::MONOSPACE).color(TEXT_BODY);
                let value = row![
                    iced::widget::tooltip(shown, container(text(address.as_str()).size(11.5).font(iced::Font::MONOSPACE)).padding([5, 8]).style(popover), iced::widget::tooltip::Position::Top),
                    button(icon(icons::copy(), 12, MUTED)).padding(4).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press(msg(TxMsg::Copy(address.clone()))),
                ]
                .spacing(4)
                .align_y(Alignment::Center);
                container(row![text(label.as_str()).size(12.5).color(TEXT_SECONDARY).width(Length::FillPortion(2)), container(value).width(Length::FillPortion(3)).align_right(Fill)].spacing(12).align_y(Alignment::Center))
                    .padding([4, 0])
                    .into()
            }
            Item::Row { label, value, strong, mono } => {
                let v = text(value.as_str()).size(if *strong { 15.0 } else { 13.0 }).color(if *strong { Color::WHITE } else { TEXT_BODY });
                let v = if *strong { v.font(BOLD) } else if *mono { v.font(iced::Font::MONOSPACE) } else { v };
                container(row![text(label.as_str()).size(12.5).color(TEXT_SECONDARY).width(Length::FillPortion(2)), container(v).width(Length::FillPortion(3)).align_right(Fill)].spacing(12).align_y(Alignment::Center))
                    .padding([7, 0])
                    .into()
            }
        };
        col = col.push(line);
    }
    col.into()
}

/// The band above the buttons: the error, the warning or the result.
fn status<'a, J>(st: &'a State<J>) -> Option<El<'a>> {
    let line = |ic: iced::widget::svg::Handle, color: Color, bg: Color, t: String| -> El<'a> {
        container(row![icon(ic, 14, color), text(t).size(12.5).color(color)].spacing(8).align_y(Alignment::Center)).width(Fill).padding([9, 12]).style(boxed(bg, Color::TRANSPARENT, 8.0)).into()
    };
    if let Some(e) = &st.error {
        let t = if st.hash.is_some() { format!("The transaction was sent, but: {e}") } else { e.clone() };
        return Some(line(icons::triangle_alert(), DANGER_TEXT, rgba(0xef4444, 0.08), t));
    }
    let f = st.finished.as_ref()?;
    if !f.saved {
        return Some(line(
            icons::triangle_alert(),
            AMBER_TEXT,
            rgba(0xf59e0b, 0.08),
            "It's live on-chain, but Accordo couldn't save it yet. It's kept on this computer and saved again when Accordo answers — don't delete the app's data folder.".into(),
        ));
    }
    if let Some(w) = &f.warning {
        return Some(line(icons::triangle_alert(), AMBER_TEXT, rgba(0xf59e0b, 0.08), w.clone()));
    }
    Some(line(icons::circle_check(), GREEN, rgba(0x10b981, 0.08), st.review.done_title.to_string()))
}

pub fn view<'a, J>(st: &'a State<J>) -> El<'a> {
    let side = container(
        column![
            row![
                container(icon(icons::shield(), 18, INDIGO_SOFT)).width(36).height(36).center(36).style(boxed(rgba(0x6366f1, 0.14), rgba(0x6366f1, 0.3), 10.0)),
                column![text(st.review.title.as_str()).size(16).font(BOLD).color(Color::WHITE), text(st.review.subtitle.as_str()).size(11.5).color(TEXT_SECONDARY)].spacing(2),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            steps(st),
            space().height(Fill),
            text("The Accordo wallet signs on this computer. Nothing is signed before you confirm.").size(11).color(FAINT),
        ]
        .spacing(18),
    )
    .width(270)
    .height(Fill)
    .padding([20, 14])
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0));

    let mut hash_row: Option<El> = None;
    if let Some(h) = st.hash.filter(|h| !h.is_zero()) {
        let short = crate::wallets::short_address(&format!("{h:#x}"));
        let mut r = row![text("Transaction").size(12).color(TEXT_SECONDARY), space().width(Fill), text(short).size(12).font(iced::Font::MONOSPACE).color(TEXT_BODY)].spacing(8).align_y(Alignment::Center);
        r = r.push(button(icon(icons::copy(), 12, MUTED)).padding(4).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press(msg(TxMsg::CopyHash)));
        if st.explorer.is_some() {
            r = r.push(button(icon(icons::external_link(), 12, MUTED)).padding(4).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press(msg(TxMsg::OpenTx)));
        }
        hash_row = Some(r.into());
    }

    let running = st.running();
    let label = match st.step {
        Step::Review => st.review.confirm.to_string(),
        Step::Approving => "Giving permission…".into(),
        Step::Signing => "Signing…".into(),
        Step::Confirming => "Confirming…".into(),
        Step::Saving => "Saving…".into(),
        Step::Done => "Done".into(),
    };
    let footer: El = if st.step == Step::Review && st.error.is_none() {
        row![
            space().width(Fill),
            button(text("Cancel").size(13)).padding([8, 16]).style(tone_button(Tone::Dark)).on_press(msg(TxMsg::Close)),
            button(row![icon(icons::shield_check(), 14, Color::WHITE), text(label).size(13).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center)).padding([8, 18]).style(tone_button(Tone::Accent)).on_press(msg(TxMsg::Confirm)),
        ]
        .spacing(8)
        .into()
    } else if running {
        row![text("Keep the app open until it's done.").size(11.5).color(FAINT), space().width(Fill), button(text(label).size(13).font(SEMIBOLD)).padding([8, 18]).style(tone_button(Tone::Accent))].align_y(Alignment::Center).into()
    } else {
        row![space().width(Fill), button(text(if st.error.is_some() { "Close" } else { "Done" }).size(13).font(SEMIBOLD)).padding([8, 18]).style(tone_button(if st.error.is_some() { Tone::Dark } else { Tone::Accent })).on_press(msg(TxMsg::Close))].into()
    };

    let mut bottom: Column<'a, Message> = column![].spacing(10);
    if let Some(s) = status(st) {
        bottom = bottom.push(s);
    }
    if let Some(h) = hash_row {
        bottom = bottom.push(h);
    }
    bottom = bottom.push(footer);

    let header = row![
        text("Details").size(12).font(BOLD).color(FAINT),
        space().width(Fill),
        button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press_maybe((!running).then(|| msg(TxMsg::Close))),
    ]
    .align_y(Alignment::Center);
    let main = column![
        container(header).padding(Padding { top: 14.0, right: 14.0, bottom: 4.0, left: 28.0 }),
        scrollable(container(items(&st.review.items)).width(Fill).padding(Padding { top: 0.0, right: 28.0, bottom: 16.0, left: 28.0 })).height(Fill).style(thin_scroll),
        container(space().width(Fill).height(1)).style(rule),
        container(bottom).padding([12, 20]),
    ]
    .width(Fill);
    let window = container(row![side, main]).width(Length::Fixed(860.0)).height(Length::Fixed(600.0)).style(modal);
    overlay::modal(window, if running { Message::None } else { msg(TxMsg::Close) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review() -> Review {
        Review { title: "t".into(), subtitle: String::new(), confirm: "Create", items: vec![], approve: true, record: true, done_title: "ok", gasless: false }
    }

    #[test]
    fn steps_go_forward_and_stop_at_the_end() {
        let mut st = State::new(review(), None, ());
        assert!(!st.running());
        st.step = Step::Approving;
        st.apply(&TxMsg::Stage(Stage::Signing, None));
        let h = B256::repeat_byte(1);
        st.apply(&TxMsg::Stage(Stage::Confirming, Some(h)));
        assert!(st.running() && st.hash == Some(h));
        st.apply(&TxMsg::Done(Ok(Finished { hash: h, saved: true, warning: None, id: None })));
        assert!(!st.running() && st.step == Step::Done);
        // A late progress message changes nothing.
        st.apply(&TxMsg::Stage(Stage::Signing, None));
        assert_eq!(st.step, Step::Done);
    }

    #[test]
    fn a_failure_keeps_the_hash() {
        let mut st = State::new(review(), Some("https://x.io/".into()), ());
        st.step = Step::Confirming;
        let h = B256::repeat_byte(2);
        st.apply(&TxMsg::Done(Err(Failed { message: "reverted".into(), hash: Some(h) })));
        assert!(!st.running());
        assert_eq!(st.tx_url().unwrap(), format!("https://x.io/tx/{h:#x}"));
    }
}
