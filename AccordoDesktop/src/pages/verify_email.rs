// SPDX-License-Identifier: GPL-3.0-or-later
//! Confirm your email (VerifyEmailGate.tsx). An account with an unconfirmed email can't use
//! Accordo yet: the server answers only /me and the confirming calls (backend security/deps.py).
//! So this screen stands in for the whole app until the 6-digit code from the email (or an access
//! code from the team) is entered; "Log out" is the other way out.
//!
//! The code goes out by itself only at sign-up and lasts 10 minutes. Signing in later sends none,
//! so "Send a new code" is always on offer.

use std::time::{Duration, Instant};

use iced::alignment::Horizontal;
use iced::widget::{button, center, column, container, row, space, text, text_input};
use iced::{Alignment, Color, Fill, Font, Task};

use crate::api::auth::{self, Session, Status, User};
use crate::api::client::ApiError;
use crate::app::{Message, SessionMsg};
use crate::components::ui::El;
use crate::theme::*;

pub const FIELD_CODE: &str = "verify-email-code";
/// "Send a new code" rests this long after sending (as on the web; the server limits it too).
const RESEND_PAUSE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub enum VerifyMsg {
    Code(String),
    /// Between the emailed code and an access code from the team.
    ToggleAccess,
    Submit,
    /// The code was taken, and `/me` after it — or why not.
    Done(Result<Box<User>, ApiError>),
    Resend,
    Resent(Result<Status, ApiError>),
}

#[derive(Default)]
pub struct State {
    pub code: String,
    pub use_access: bool,
    pub busy: bool,
    sent_at: Option<Instant>,
}

impl State {
    fn ready(&self) -> bool {
        let code = self.code.trim();
        if self.use_access {
            code.len() >= 12
        } else {
            code.len() == 6 && code.chars().all(|c| c.is_ascii_digit())
        }
    }

    fn resting(&self) -> bool {
        self.sent_at.is_some_and(|t| t.elapsed() < RESEND_PAUSE)
    }
}

/// What the app should do after one of this screen's messages.
pub enum Outcome {
    None,
    Task(Task<Message>),
    /// The account is confirmed: into the app, with the user as the server has it now.
    Confirmed(Box<User>),
    Error(String),
    Notice(String),
}

fn m(msg: VerifyMsg) -> Message {
    Message::Verify(msg)
}

pub fn update(state: &mut State, session: &Session, msg: VerifyMsg) -> Outcome {
    let api = session.api();
    match msg {
        VerifyMsg::Code(v) => {
            state.code = if state.use_access { v.to_uppercase().chars().take(20).collect() } else { v.chars().filter(char::is_ascii_digit).take(6).collect() };
        }
        VerifyMsg::ToggleAccess => {
            state.use_access = !state.use_access;
            state.code.clear();
            return Outcome::Task(iced::widget::operation::focus(FIELD_CODE));
        }
        VerifyMsg::Submit => {
            if state.busy || !state.ready() {
                return Outcome::None;
            }
            state.busy = true;
            let code = state.code.trim().to_string();
            let access = state.use_access;
            let call = async move {
                if access {
                    auth::verify_with_access_code(api.clone(), code).await?;
                } else {
                    auth::verify_email(api.clone(), code).await?;
                }
                auth::me(api).await.map(Box::new)
            };
            return Outcome::Task(Task::perform(call, |r| m(VerifyMsg::Done(r))));
        }
        VerifyMsg::Done(result) => {
            state.busy = false;
            return match result {
                Ok(user) if !user.email_verification_required => Outcome::Confirmed(user),
                Ok(_) => Outcome::Error("Your email still shows as unconfirmed — try the code again".into()),
                Err(e) => Outcome::Error(e.message),
            };
        }
        VerifyMsg::Resend => {
            if state.resting() {
                return Outcome::None;
            }
            state.sent_at = Some(Instant::now());
            return Outcome::Task(Task::perform(auth::send_email_verification(api), |r| m(VerifyMsg::Resent(r))));
        }
        // Confirmed meanwhile (on the web, on another computer): in, as the account is now.
        VerifyMsg::Resent(Ok(s)) if s.status == "already_verified" => return Outcome::Task(Task::perform(auth::me(api), |r| m(VerifyMsg::Done(r.map(Box::new))))),
        VerifyMsg::Resent(Ok(_)) => return Outcome::Notice("New code sent".into()),
        VerifyMsg::Resent(Err(e)) => {
            state.sent_at = None;
            return Outcome::Error(e.message);
        }
    }
    Outcome::None
}

fn small_link<'a>(label: &'a str, on: Message) -> El<'a> {
    button(text(label).size(13).color(ACCENT)).padding(0).style(link_button).on_press(on).into()
}

pub fn view<'a>(state: &'a State, session: &'a Session) -> El<'a> {
    let email = session.user.email.as_deref().unwrap_or("your email");
    let (title, intro) = if state.use_access {
        ("Enter your access code", "An access code from the Accordo team confirms your account right away.".to_string())
    } else {
        ("Confirm your email", format!("Enter the 6-digit code we emailed to {email}. It lasts 10 minutes — if it ran out or never came, send a new one."))
    };
    let submit = m(VerifyMsg::Submit);
    let input = text_input(if state.use_access { "XXXX-XXXX-XXXX" } else { "000000" }, &state.code)
        .id(FIELD_CODE)
        .size(18)
        .padding([10, 12])
        .font(Font::MONOSPACE)
        .align_x(Horizontal::Center)
        .style(field)
        .on_input(|v| m(VerifyMsg::Code(v)))
        .on_submit(submit.clone());
    let confirm = button(center(text(if state.busy { "Confirming…" } else { "Confirm" }).size(14).font(MEDIUM)).height(20))
        .width(Fill)
        .padding([10, 18])
        .style(accent_button)
        .on_press_maybe((state.ready() && !state.busy).then_some(submit));
    let resend: El = if state.use_access {
        space().into()
    } else if state.resting() {
        text("Code sent — check spam too").size(13).color(TEXT_SECONDARY).into()
    } else {
        small_link("Send a new code", m(VerifyMsg::Resend))
    };
    let toggle = small_link(if state.use_access { "Use the emailed code" } else { "I have an access code" }, m(VerifyMsg::ToggleAccess));
    let logout = button(text(format!("Log out of @{}", session.user.username)).size(12.5).color(TEXT_SECONDARY)).padding(0).style(link_button).on_press(Message::Session(SessionMsg::Logout));
    let form = column![
        column![text(title).size(24).font(BOLD).color(Color::WHITE), text(intro).size(13).color(TEXT_SECONDARY).line_height(1.5)].spacing(6),
        input,
        confirm,
        row![resend, space().width(Fill), toggle].align_y(Alignment::Center),
        container(logout).width(Fill).align_x(Horizontal::Center),
    ]
    .spacing(16);
    center(container(form).width(400).padding(24).style(card)).style(panel(BG_DARK)).into()
}
