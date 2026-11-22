// SPDX-License-Identifier: GPL-3.0-or-later
//! Signing up and getting back in (RegisterPage.tsx, ForgotPasswordPage.tsx), inside the app:
//!
//! - register: email (or an access code instead), username, password, the terms; invite-only and
//!   paused sign-ups offer the waitlist. A new account gets its chat key right here (made on this
//!   computer, the public half sent with the sign-up, the private one into the OS secret store);
//!   accounts without an email get recovery codes, shown once;
//! - forgot password: a 6-digit code by email, then a new password — or a recovery code.

use iced::alignment::Horizontal;
use iced::widget::{button, center, checkbox, column, container, row, space, text, text_input, Column};
use iced::{Alignment, Color, Fill, Font, Task};

use crate::api::auth::{self, Registration, RegistrationStatus, Session};
use crate::api::client::{normalize_base, Api, ApiError};
use crate::app::Message;
use crate::components::ui::{icon, El};
use crate::e2ee::{crypto::DeviceKeypair, keystore};
use crate::icons;
use crate::pages::login::{LoginMsg, Outcome};
use crate::theme::*;

/// Terms of Service + Privacy Policy version (web: legal.ts TERMS_VERSION).
pub const TERMS_VERSION: &str = "2026-10-02";
const MIN_PASSWORD: usize = 8;

#[derive(Debug, Clone)]
pub enum FormMsg {
    // register
    Email(String),
    Username(String),
    Password(String),
    Confirm(String),
    ToggleShow,
    UseCode(bool),
    AccessCode(String),
    Agree(bool),
    Register,
    Status(Result<RegistrationStatus, ApiError>),
    Registered(Result<Box<Registered>, ApiError>),
    Waitlist(String),
    JoinWaitlist,
    Joined(Result<(), ApiError>),
    CopyCodes,
    CodesSaved,
    OpenTerms,
    OpenPrivacy,
    // forgot password
    SendCode,
    CodeSent(Result<(), ApiError>),
    Code(String),
    Login(String),
    RecoveryCode(String),
    Reset,
    ResetDone(Result<Option<u32>, ApiError>),
    UseRecovery(bool),
    ChangeEmail,
}

#[derive(Debug, Clone)]
pub struct Registered {
    pub session: Session,
    pub recovery_codes: Option<Vec<String>>,
    /// The chat key couldn't be stored on this computer.
    pub key_warning: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgotStep {
    Email,
    Code,
    Recovery,
}

#[derive(Default)]
pub struct RegisterState {
    pub status: Option<RegistrationStatus>,
    pub email: String,
    pub username: String,
    pub password: String,
    pub show: bool,
    pub use_code: bool,
    pub access_code: String,
    pub agreed: bool,
    pub busy: bool,
    pub waitlist_email: String,
    pub waitlist_open: bool,
    pub joined: bool,
    /// After an email-less sign-up: the codes, and the session to enter with once they're saved.
    pub codes: Option<(Vec<String>, Session)>,
}

pub struct ForgotState {
    pub step: ForgotStep,
    pub email: String,
    pub code: String,
    pub login: String,
    pub recovery: String,
    pub password: String,
    pub confirm: String,
    pub show: bool,
    pub busy: bool,
}

impl ForgotState {
    pub fn new(email: String) -> Self {
        Self { step: ForgotStep::Email, email, code: String::new(), login: String::new(), recovery: String::new(), password: String::new(), confirm: String::new(), show: false, busy: false }
    }
}

pub enum Screen {
    SignIn,
    Register(Box<RegisterState>),
    Forgot(Box<ForgotState>),
}

fn m(f: FormMsg) -> Message {
    Message::Login(LoginMsg::Form(f))
}

impl RegisterState {
    fn invite_only(&self) -> bool {
        self.status.as_ref().is_some_and(|s| s.remaining_spots == Some(0) || s.invite_code_required)
    }

    fn paused(&self) -> bool {
        self.status.as_ref().is_some_and(|s| !s.enabled)
    }

    /// An access-code sign-up has no email (the account starts verified).
    fn with_code(&self) -> bool {
        self.use_code || self.invite_only()
    }
}

pub fn open_register(base: &str) -> (Screen, Task<Message>) {
    let api = Api::guest(&normalize_base(base));
    (Screen::Register(Box::default()), Task::perform(auth::registration_status(api), |r| m(FormMsg::Status(r))))
}

/// Sign up with a chat key made here: the public half goes with the account, the private one
/// into the secret store under the new account's id.
async fn register(base: String, form: Registration, keypair: DeviceKeypair) -> Result<Box<Registered>, ApiError> {
    let res = auth::register(Api::guest(&base), form).await?;
    let account = crate::store::app_store::account_key(res.user.id, &res.user.username, &base);
    let key_warning = keystore::blocking(move || keystore::save(&account, &keypair)).await.err();
    Ok(Box::new(Registered { session: Session { base, token: res.access_token, user: res.user }, recovery_codes: res.recovery_codes, key_warning }))
}

fn check_passwords(password: &str, confirm: &str) -> Result<(), String> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(format!("Password must be at least {MIN_PASSWORD} characters"));
    }
    if password != confirm {
        return Err("Passwords don't match".into());
    }
    Ok(())
}

pub fn update(screen: &mut Screen, base: &str, msg: FormMsg) -> Outcome {
    let base = normalize_base(base);
    let api = Api::guest(&base);
    match screen {
        Screen::Register(r) => match msg {
            FormMsg::Status(Ok(s)) => r.status = Some(s),
            FormMsg::Status(Err(_)) => {}
            FormMsg::Email(v) => r.email = v,
            FormMsg::Username(v) => r.username = v.trim().trim_start_matches('@').to_string(),
            FormMsg::Password(v) => r.password = v,
            FormMsg::ToggleShow => r.show = !r.show,
            FormMsg::UseCode(v) => r.use_code = v,
            FormMsg::AccessCode(v) => r.access_code = v.to_uppercase().chars().take(20).collect(),
            FormMsg::Agree(v) => r.agreed = v,
            FormMsg::Register => {
                if r.busy {
                    return Outcome::None;
                }
                if !r.agreed {
                    return Outcome::Error("Please accept the Terms of Service and Privacy Policy".into());
                }
                if r.username.is_empty() {
                    return Outcome::Error("Choose a username".into());
                }
                if r.password.chars().count() < MIN_PASSWORD {
                    return Outcome::Error(format!("Password must be at least {MIN_PASSWORD} characters"));
                }
                let with_code = r.with_code();
                if with_code && r.access_code.trim().is_empty() {
                    return Outcome::Error("Enter your access code".into());
                }
                if !with_code && r.email.trim().is_empty() {
                    return Outcome::Error("Enter your email".into());
                }
                r.busy = true;
                let keypair = DeviceKeypair::generate();
                let form = Registration {
                    email: (!with_code).then(|| r.email.trim().to_string()),
                    username: r.username.clone(),
                    password: r.password.clone(),
                    display_name: None,
                    e2ee_public_key: Some(keypair.public_jwk.clone()),
                    accepted_terms: Some(TERMS_VERSION.into()),
                    access_code: with_code.then(|| r.access_code.trim().to_string()),
                };
                return Outcome::Task(Task::perform(register(base, form, keypair), |res| m(FormMsg::Registered(res))));
            }
            FormMsg::Registered(res) => {
                r.busy = false;
                match res {
                    Ok(done) => {
                        let done = *done;
                        if let Some(w) = &done.key_warning {
                            tracing::warn!("sign-up: chat key not stored: {w}");
                        }
                        r.password.clear();
                        match done.recovery_codes.filter(|c| !c.is_empty()) {
                            Some(codes) => r.codes = Some((codes, done.session)),
                            None => return Outcome::SignedIn(done.session),
                        }
                    }
                    Err(e) => return Outcome::Error(e.message),
                }
            }
            FormMsg::Waitlist(v) => r.waitlist_email = v,
            FormMsg::JoinWaitlist => {
                let email = r.waitlist_email.trim().to_string();
                if email.is_empty() {
                    r.waitlist_open = true;
                    return Outcome::None;
                }
                return Outcome::Task(Task::perform(auth::join_waitlist(api, email), |res| m(FormMsg::Joined(res.map(|_| ())))));
            }
            FormMsg::Joined(Ok(())) => r.joined = true,
            FormMsg::Joined(Err(e)) => return Outcome::Error(e.message),
            FormMsg::CopyCodes => {
                if let Some((codes, _)) = &r.codes {
                    return Outcome::Task(Task::batch([iced::clipboard::write(codes.join("\n")), Task::done(Message::Login(LoginMsg::Notice("Recovery codes copied".into())))]));
                }
            }
            FormMsg::CodesSaved => {
                if let Some((_, session)) = r.codes.take() {
                    return Outcome::SignedIn(session);
                }
            }
            FormMsg::OpenTerms => return Outcome::OpenWeb(format!("{base}/terms")),
            FormMsg::OpenPrivacy => return Outcome::OpenWeb(format!("{base}/privacy")),
            _ => {}
        },
        Screen::Forgot(f) => match msg {
            FormMsg::Email(v) => f.email = v,
            FormMsg::Code(v) => f.code = v.chars().filter(char::is_ascii_digit).take(6).collect(),
            FormMsg::Login(v) => f.login = v,
            FormMsg::RecoveryCode(v) => f.recovery = v.to_uppercase().chars().take(16).collect(),
            FormMsg::Password(v) => f.password = v,
            FormMsg::Confirm(v) => f.confirm = v,
            FormMsg::ToggleShow => f.show = !f.show,
            FormMsg::UseRecovery(on) => f.step = if on { ForgotStep::Recovery } else { ForgotStep::Email },
            FormMsg::ChangeEmail => {
                f.step = ForgotStep::Email;
                f.code.clear();
            }
            FormMsg::SendCode => {
                if f.busy {
                    return Outcome::None;
                }
                if f.email.trim().is_empty() {
                    return Outcome::Error("Enter your account email".into());
                }
                f.busy = true;
                return Outcome::Task(Task::perform(auth::forgot_password(api, f.email.trim().to_string()), |res| m(FormMsg::CodeSent(res.map(|_| ())))));
            }
            FormMsg::CodeSent(res) => {
                f.busy = false;
                match res {
                    Ok(()) => f.step = ForgotStep::Code,
                    Err(e) => return Outcome::Error(e.message),
                }
            }
            FormMsg::Reset => {
                if f.busy {
                    return Outcome::None;
                }
                if let Err(e) = check_passwords(&f.password, &f.confirm) {
                    return Outcome::Error(e);
                }
                f.busy = true;
                let call = match f.step {
                    ForgotStep::Recovery => {
                        if f.login.trim().is_empty() || f.recovery.trim().is_empty() {
                            f.busy = false;
                            return Outcome::Error("Enter your username or email and a recovery code".into());
                        }
                        Task::perform(auth::recover_password(api, f.login.trim().to_string(), f.recovery.trim().to_string(), f.password.clone()), |res| m(FormMsg::ResetDone(res.map(|s| s.recovery_codes_left))))
                    }
                    _ => {
                        if f.code.len() != 6 {
                            f.busy = false;
                            return Outcome::Error("Enter the 6-digit code from the email".into());
                        }
                        Task::perform(auth::reset_password(api, f.email.trim().to_string(), f.code.clone(), f.password.clone()), |res| m(FormMsg::ResetDone(res.map(|_| None))))
                    }
                };
                return Outcome::Task(call);
            }
            FormMsg::ResetDone(res) => {
                f.busy = false;
                match res {
                    Ok(left) => {
                        let note = match left {
                            Some(n) => format!("Password updated — sign in with your new password. {n} recovery code(s) left."),
                            None => "Password updated — sign in with your new password. Other sessions were signed out.".into(),
                        };
                        return Outcome::BackToSignIn(note);
                    }
                    Err(e) => return Outcome::Error(e.message),
                }
            }
            _ => {}
        },
        Screen::SignIn => {}
    }
    Outcome::None
}

// ── views ──

fn heading<'a>(title: &'a str, sub: String) -> El<'a> {
    column![text(title).size(24).font(BOLD).color(Color::WHITE), text(sub).size(13).color(TEXT_SECONDARY).line_height(1.5)].spacing(6).into()
}

fn field_input<'a>(placeholder: &'a str, value: &'a str, on: fn(String) -> FormMsg, submit: FormMsg) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value).size(14).padding([10, 12]).style(field).on_input(move |v| m(on(v))).on_submit(m(submit))
}

fn secret<'a>(placeholder: &'a str, value: &'a str, show: bool, on: fn(String) -> FormMsg, submit: FormMsg, eye: bool) -> El<'a> {
    let input = field_input(placeholder, value, on, submit).secure(!show);
    if !eye {
        return input.into();
    }
    iced::widget::stack![
        input,
        container(button(icon(if show { icons::eye_off() } else { icons::eye() }, 16, MUTED)).padding([4, 6]).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(m(FormMsg::ToggleShow)))
            .width(Fill)
            .height(iced::Length::Fixed(41.0))
            .align_x(Horizontal::Right)
            .align_y(iced::alignment::Vertical::Center)
            .padding([0, 6]),
    ]
    .into()
}

fn primary_button<'a>(label: &'a str, busy_label: &'a str, busy: bool, enabled: bool, on: FormMsg) -> El<'a> {
    button(center(text(if busy { busy_label } else { label }).size(14).font(MEDIUM)).height(20))
        .width(Fill)
        .padding([10, 18])
        .style(accent_button)
        .on_press_maybe((enabled && !busy).then(|| m(on)))
        .into()
}

fn small_link<'a>(label: &'a str, on: Message) -> El<'a> {
    button(text(label).size(13).color(ACCENT)).padding(0).style(link_button).on_press(on).into()
}

fn hint<'a>(s: &'a str) -> El<'a> {
    text(s).size(12).color(TEXT_SECONDARY).line_height(1.5).into()
}

fn waitlist<'a>(r: &'a RegisterState) -> El<'a> {
    if r.joined {
        return text("You're on the list. When your spot comes up, we'll email you an invitation with an access code.").size(12.5).color(GREEN).line_height(1.5).into();
    }
    row![
        text_input("Your email", &r.waitlist_email).size(13).padding([9, 10]).style(field).on_input(|v| m(FormMsg::Waitlist(v))).on_submit(m(FormMsg::JoinWaitlist)),
        button(text("Join waitlist").size(13)).padding([9, 12]).style(secondary).on_press_maybe((!r.waitlist_email.trim().is_empty()).then(|| m(FormMsg::JoinWaitlist))),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn sign_in_link<'a>() -> El<'a> {
    container(row![text("Already have an account?").size(13).color(TEXT_SECONDARY), small_link("Sign in", Message::Login(LoginMsg::ShowSignIn))].spacing(5).align_y(Alignment::Center)).width(Fill).align_x(Horizontal::Center).into()
}

pub fn register_view(r: &RegisterState) -> El<'_> {
    if let Some((codes, session)) = &r.codes {
        let mut grid = Column::new().spacing(6);
        for pair in codes.chunks(2) {
            let mut line = row![].spacing(6);
            for c in pair {
                line = line.push(container(text(c.clone()).size(15).font(Font::MONOSPACE).color(Color::WHITE)).width(Fill).padding([8, 10]).align_x(Horizontal::Center).style(boxed(hex(0x0e0f17), BORDER, 8.0)));
            }
            grid = grid.push(line);
        }
        return column![
            heading("Save your recovery codes", format!("@{} has no email, so these codes are the only way back in if you forget your password. Each works once.", session.user.username)),
            grid,
            row![
                button(row![icon(icons::copy(), 14, Color::WHITE), text("Copy all").size(13).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center)).padding([9, 14]).style(secondary).on_press(m(FormMsg::CopyCodes)),
                space().width(Fill),
            ],
            primary_button("I've saved them — go to Accordo", "", false, true, FormMsg::CodesSaved),
        ]
        .spacing(16)
        .into();
    }
    if r.paused() {
        return column![
            heading("Sign-ups are paused", "New accounts are closed for the moment. Leave your email and we'll send you an invitation when they reopen.".into()),
            waitlist(r),
            sign_in_link(),
        ]
        .spacing(16)
        .into();
    }
    let with_code = r.with_code();
    let mut form = column![heading("Create an account", "Your @username is how people find and pay you — it can't be changed later.".into())].spacing(14);
    if r.invite_only() {
        form = form.push(container(hint("Accordo is invite-only right now — sign up with an access code from an admin or a friend.")).padding([10, 12]).style(boxed(rgba(0x6366f1, 0.08), rgba(0x6366f1, 0.25), 10.0)));
    }
    if !with_code {
        form = form.push(field_input("Email", &r.email, FormMsg::Email, FormMsg::Register));
    }
    form = form
        .push(field_input("Username", &r.username, FormMsg::Username, FormMsg::Register))
        .push(secret("Password (min. 8 characters)", &r.password, r.show, FormMsg::Password, FormMsg::Register, true));
    if !r.invite_only() {
        form = form.push(checkbox(r.use_code).label("I have an access code").size(15).text_size(12.5).style(tick).on_toggle(|v| m(FormMsg::UseCode(v))));
    }
    if with_code {
        form = form
            .push(field_input("Access code", &r.access_code, FormMsg::AccessCode, FormMsg::Register).font(Font::MONOSPACE))
            .push(hint("With a code you sign up with just a username. You get recovery codes for a forgotten password and can add an email later in Settings."));
    }
    form = form
        .push(
            row![
                checkbox(r.agreed).size(15).style(tick).on_toggle(|v| m(FormMsg::Agree(v))),
                text("I'm 18 or older and agree to the").size(12.5).color(TEXT_SECONDARY),
                small_link("Terms", m(FormMsg::OpenTerms)),
                text("and").size(12.5).color(TEXT_SECONDARY),
                small_link("Privacy Policy", m(FormMsg::OpenPrivacy)),
            ]
            .spacing(5)
            .align_y(Alignment::Center)
            .wrap(),
        )
        .push(primary_button("Register", "Creating account…", r.busy, r.agreed, FormMsg::Register));
    if r.invite_only() {
        form = form.push(if r.waitlist_open { waitlist(r) } else { container(row![text("No code?").size(13).color(TEXT_SECONDARY), small_link("Join the waitlist", m(FormMsg::JoinWaitlist))].spacing(5)).width(Fill).align_x(Horizontal::Center).into() });
    }
    form.push(sign_in_link()).into()
}

pub fn forgot_view(f: &ForgotState) -> El<'_> {
    let passwords = |label_submit: FormMsg| -> El<'_> {
        column![
            secret("New password (min 8 characters)", &f.password, f.show, FormMsg::Password, label_submit.clone(), true),
            secret("Confirm new password", &f.confirm, f.show, FormMsg::Confirm, label_submit, false),
        ]
        .spacing(12)
        .into()
    };
    let footer = |left: El<'static>, right: El<'static>| -> El<'static> { row![left, space().width(Fill), right].align_y(Alignment::Center).into() };
    match f.step {
        ForgotStep::Email => column![
            heading("Reset password", "Enter your account email and we'll send a 6-digit code to reset your password.".into()),
            field_input("Email", &f.email, FormMsg::Email, FormMsg::SendCode),
            primary_button("Send reset code", "Sending…", f.busy, !f.email.trim().is_empty(), FormMsg::SendCode),
            footer(small_link("No email? Use a recovery code", m(FormMsg::UseRecovery(true))), small_link("Sign in", Message::Login(LoginMsg::ShowSignIn))),
        ]
        .spacing(16)
        .into(),
        ForgotStep::Code => column![
            heading("Enter your code", format!("If an account exists for {}, a code is on its way. It expires in 10 minutes.", f.email.trim())),
            field_input("6-digit code", &f.code, FormMsg::Code, FormMsg::Reset).font(Font::MONOSPACE).size(18).align_x(Horizontal::Center),
            passwords(FormMsg::Reset),
            primary_button("Set new password", "Saving…", f.busy, f.code.len() == 6, FormMsg::Reset),
            footer(small_link("Change email", m(FormMsg::ChangeEmail)), small_link("Resend code", m(FormMsg::SendCode))),
        ]
        .spacing(16)
        .into(),
        ForgotStep::Recovery => column![
            heading("Use a recovery code", "One of the codes you saved when you created your account or turned on two-factor login. Each code works once; all your sessions will be signed out.".into()),
            field_input("Username or email", &f.login, FormMsg::Login, FormMsg::Reset),
            field_input("Recovery code (XXXXX-XXXXX)", &f.recovery, FormMsg::RecoveryCode, FormMsg::Reset).font(Font::MONOSPACE).align_x(Horizontal::Center),
            passwords(FormMsg::Reset),
            primary_button("Set new password", "Saving…", f.busy, true, FormMsg::Reset),
            footer(small_link("Reset by email instead", m(FormMsg::UseRecovery(false))), small_link("Sign in", Message::Login(LoginMsg::ShowSignIn))),
        ]
        .spacing(16)
        .into(),
    }
}
