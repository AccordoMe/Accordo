// SPDX-License-Identifier: GPL-3.0-or-later
//! Sign in (LoginPage.tsx): email or username + password, a second-factor code when the
//! account has 2FA, links to password reset / registration, and the backend address.

use iced::alignment::Horizontal;
use iced::widget::{button, center, column, container, rich_text, row, span, space, text, text_input};
use iced::{Alignment, Color, Fill, Font, Task};

use crate::api::auth::{self, Session, TWO_FACTOR_REQUIRED};
use crate::api::client::{normalize_base, ApiError, DEFAULT_API_URL};
use crate::app::{Message, SessionMsg};
use crate::components::ui::El;
use crate::store::app_store::SavedAccount;
use crate::theme::{self, *};

pub const FIELD_ID: &str = "login-identifier";
pub const FIELD_PASSWORD: &str = "login-password";
const FIELD_OTP: &str = "login-otp";

#[derive(Debug, Clone)]
pub enum LoginMsg {
    Identifier(String),
    Password(String),
    Otp(String),
    ApiUrl(String),
    ResetApiUrl,
    Submit,
    Finished(Result<Session, ApiError>),
    ForgotPassword,
    Register,
    /// Show / hide the password (to check what was pasted).
    ToggleShow,
    /// Registration / password reset (auth_forms).
    Form(super::auth_forms::FormMsg),
    ShowSignIn,
    /// Something done, to show (a toast).
    Notice(String),
}

pub struct State {
    pub identifier: String,
    pub password: String,
    /// Set once the server says the password was right but a second factor is needed.
    pub need_otp: bool,
    pub otp: String,
    pub submitting: bool,
    pub api_url: String,
    pub show_password: bool,
    pub screen: super::auth_forms::Screen,
    /// Adding an account: the one to go back to (its key).
    pub back_to: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            identifier: String::new(),
            password: String::new(),
            need_otp: false,
            otp: String::new(),
            submitting: false,
            api_url: default_api_url(),
            show_password: false,
            screen: super::auth_forms::Screen::SignIn,
            back_to: None,
        }
    }
}

fn default_api_url() -> String {
    std::env::var("ACCORDO_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string())
}

/// What the app should do after a sign-in event.
pub enum Outcome {
    None,
    Task(Task<Message>),
    SignedIn(Session),
    Error(String),
    /// Open a page of the web app in the browser (terms, privacy).
    OpenWeb(String),
    /// Something worked: say so.
    Notice(String),
    /// A new password was set: back to signing in, with a note.
    BackToSignIn(String),
}

pub fn update(state: &mut State, msg: LoginMsg) -> Outcome {
    match msg {
        LoginMsg::Identifier(v) => {
            // Another account: its second factor isn't asked for yet.
            state.identifier = v;
            state.need_otp = false;
            state.otp.clear();
        }
        LoginMsg::Password(v) => state.password = v,
        LoginMsg::ToggleShow => state.show_password = !state.show_password,
        LoginMsg::Otp(v) => state.otp = v.to_uppercase().chars().take(16).collect(),
        LoginMsg::ApiUrl(v) => state.api_url = v,
        LoginMsg::ResetApiUrl => state.api_url = default_api_url(),
        LoginMsg::Submit => {
            if state.submitting {
                return Outcome::None;
            }
            if state.identifier.trim().is_empty() || state.password.is_empty() {
                return Outcome::Error("Enter your email or username and password".into());
            }
            if state.need_otp && state.otp.trim().is_empty() {
                return Outcome::Error("Enter the code from your authenticator app".into());
            }
            state.submitting = true;
            let otp = state.need_otp.then(|| state.otp.trim().to_string());
            // The server matches the login exactly: a space pasted along with it would read as
            // "Invalid credentials". The password goes as typed (spaces may be part of it).
            let call = auth::sign_in(normalize_base(&state.api_url), state.identifier.trim().to_string(), state.password.clone(), otp);
            return Outcome::Task(Task::perform(call, |r| Message::Login(LoginMsg::Finished(r))));
        }
        LoginMsg::Finished(result) => {
            state.submitting = false;
            match result {
                Ok(session) => {
                    state.password.clear();
                    state.otp.clear();
                    state.need_otp = false;
                    return Outcome::SignedIn(session);
                }
                Err(e) if e.status == Some(403) && e.message == TWO_FACTOR_REQUIRED => {
                    state.need_otp = true;
                    return Outcome::Task(iced::widget::operation::focus(FIELD_OTP));
                }
                Err(e) => return Outcome::Error(e.message),
            }
        }
        LoginMsg::ForgotPassword => {
            // Start with what was typed, if it looks like an email.
            let email = if state.identifier.contains('@') { state.identifier.trim().to_string() } else { String::new() };
            state.screen = super::auth_forms::Screen::Forgot(Box::new(super::auth_forms::ForgotState::new(email)));
        }
        LoginMsg::Register => {
            let (screen, task) = super::auth_forms::open_register(&state.api_url);
            state.screen = screen;
            return Outcome::Task(task);
        }
        LoginMsg::ShowSignIn => state.screen = super::auth_forms::Screen::SignIn,
        LoginMsg::Notice(n) => return Outcome::Notice(n),
        LoginMsg::Form(f) => {
            let outcome = super::auth_forms::update(&mut state.screen, &state.api_url, f);
            match outcome {
                Outcome::BackToSignIn(note) => {
                    state.screen = super::auth_forms::Screen::SignIn;
                    state.password.clear();
                    return Outcome::Notice(note);
                }
                Outcome::SignedIn(session) => {
                    state.screen = super::auth_forms::Screen::SignIn;
                    return Outcome::SignedIn(session);
                }
                other => return other,
            }
        }
    }
    Outcome::None
}

/// `accounts`: the ones signed in on this computer, most recent first (open one, or sign in to
/// one that needs it again).
pub fn view<'a>(state: &'a State, accounts: Vec<&'a SavedAccount>) -> El<'a> {
    use super::auth_forms::{forgot_view, register_view, Screen};
    let other: Option<El> = match &state.screen {
        Screen::SignIn => None,
        Screen::Register(r) => Some(column![register_view(r), api_endpoint(state)].spacing(16).into()),
        Screen::Forgot(f) => Some(column![forgot_view(f), api_endpoint(state)].spacing(16).into()),
    };
    if let Some(form) = other {
        return center(container(iced::widget::scrollable(form).height(iced::Length::Shrink)).width(400).padding(24).style(card)).style(panel(BG_DARK)).into();
    }
    let submit = Message::Login(LoginMsg::Submit);
    let input = |placeholder: &str, value: &str, id: &'static str| {
        text_input(placeholder, value).id(id).size(14).padding([10, 12]).style(field).on_submit(submit.clone())
    };

    let mut form = column![];
    if let Some(back) = state.back_to.as_deref().and_then(|k| accounts.iter().find(|a| a.key() == k)) {
        form = form.push(
            button(row![crate::components::ui::icon(crate::icons::arrow_left(), 13, TEXT_SECONDARY), text(format!("Back to @{}", back.username)).size(12.5).color(TEXT_SECONDARY)].spacing(6).align_y(Alignment::Center))
                .padding(0)
                .style(link_button)
                .on_press(Message::Session(SessionMsg::SwitchTo(back.key()))),
        );
    }
    let mut form = form.push(column![
        text(if state.back_to.is_some() { "Add an account" } else { "Sign in" }).size(24).font(BOLD).color(Color::WHITE),
        space().height(4),
        input("Email or Username", &state.identifier, FIELD_ID).on_input(|v| Message::Login(LoginMsg::Identifier(v))),
        iced::widget::stack![
            input("Password", &state.password, FIELD_PASSWORD).secure(!state.show_password).on_input(|v| Message::Login(LoginMsg::Password(v))),
            iced::widget::container(
                iced::widget::button(crate::components::ui::icon(if state.show_password { crate::icons::eye_off() } else { crate::icons::eye() }, 16, MUTED))
                    .padding([4, 6])
                    .style(ghost(6.0, rgba(0xffffff, 0.06)))
                    .on_press(Message::Login(LoginMsg::ToggleShow)),
            )
            .width(Fill)
            .height(iced::Length::Fixed(41.0))
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Center)
            .padding([0, 6]),
        ],
    ]
    .spacing(16))
    .spacing(16);

    if state.need_otp {
        form = form
            .push(
                text("Enter the 6-digit code from your authenticator app — or one of your recovery codes if you don't have your phone.")
                    .size(12.5)
                    .color(TEXT_SECONDARY)
                    .line_height(1.5),
            )
            .push(
                input("123456 or recovery code", &state.otp, FIELD_OTP)
                    .on_input(|v| Message::Login(LoginMsg::Otp(v)))
                    .font(Font::MONOSPACE)
                    .size(16)
                    .align_x(Horizontal::Center),
            );
    }

    form = form
        .push(container(link("Forgot password?", 12.5, Message::Login(LoginMsg::ForgotPassword))).width(Fill).align_x(Horizontal::Right))
        .push(
            button(center(text(if state.submitting { "Signing in…" } else { "Sign in" }).size(14).font(MEDIUM)).height(20))
                .width(Fill)
                .padding([10, 18])
                .style(accent_button)
                .on_press_maybe((!state.submitting).then_some(submit.clone())),
        )
        .push(
            container(
                row![
                    text("No account?").size(14).color(TEXT_SECONDARY),
                    button(text("Register").size(14).color(ACCENT)).padding(0).style(link_button).on_press(Message::Login(LoginMsg::Register)),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .align_x(Horizontal::Center),
        )
        .push(api_endpoint(state));

    // Adding one: the others are a click away in the app already.
    let others: Vec<&SavedAccount> = if state.back_to.is_some() { Vec::new() } else { accounts };
    if !others.is_empty() {
        form = form.push(saved_accounts(others));
    }
    center(iced::widget::scrollable(container(form).width(380).padding(24).style(card)).height(iced::Length::Shrink)).style(panel(BG_DARK)).into()
}

/// "On this computer": accounts signed in before — open one, or sign in to it again.
fn saved_accounts<'a>(accounts: Vec<&'a SavedAccount>) -> El<'a> {
    let mut col = column![
        container(space().width(Fill).height(1)).style(theme::rule),
        space().height(4),
        text("ON THIS COMPUTER").size(10.5).font(BOLD).color(FAINT),
    ]
    .spacing(6);
    for a in accounts {
        let status = if a.signed_out { format!("{} · sign in again", a.server()) } else { a.server() };
        let open = button(
            row![
                crate::components::ui::avatar_pic(&a.username, a.avatar.as_deref(), 28.0, Some(INDIGO.into())),
                column![text(format!("@{}", a.username)).size(13).font(SEMIBOLD).color(Color::WHITE), text(status).size(11).color(if a.signed_out { AMBER_TEXT } else { TEXT_SECONDARY })].spacing(1),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .width(Fill)
        .padding([6, 8])
        .style(ghost(8.0, rgba(0xffffff, 0.05)))
        .on_press(Message::Session(SessionMsg::SwitchTo(a.key())));
        let remove = button(crate::components::ui::icon(crate::icons::x(), 12, MUTED)).padding(6).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(Message::Session(SessionMsg::Remove(a.key())));
        col = col.push(row![open, remove].spacing(4).align_y(Alignment::Center));
    }
    col.into()
}

/// "Backend API Endpoint" at the bottom of the card: which server to sign in to.
fn api_endpoint(state: &State) -> El<'_> {
    let mut label = row![text("Backend API Endpoint").size(12).color(TEXT_SECONDARY), space().width(Fill)].align_y(Alignment::Center);
    if state.api_url.trim() != default_api_url() {
        label = label.push(button(text("Reset Default").size(10).color(SKY)).padding(0).style(link_button).on_press(Message::Login(LoginMsg::ResetApiUrl)));
    }
    column![
        container(space().width(Fill).height(1)).style(theme::rule),
        space().height(8),
        label,
        text_input("e.g. https://api.accordo.me or http://127.0.0.1:8000", &state.api_url)
            .size(12)
            .padding([8, 10])
            .style(field)
            .on_input(|v| Message::Login(LoginMsg::ApiUrl(v)))
            .on_submit(Message::Login(LoginMsg::Submit)),
    ]
    .spacing(6)
    .into()
}

/// An underlined text link, like the web's <a>.
fn link<'a>(label: &'a str, size: f32, message: Message) -> El<'a> {
    button(rich_text![span::<(), Font>(label).underline(true).color(LINK)].size(size)).padding(0).style(link_button).on_press(message).into()
}
