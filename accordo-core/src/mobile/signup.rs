// SPDX-License-Identifier: GPL-3.0-or-later
//! Signing up and getting back in, for Kotlin: the same endpoints the desktop's register and
//! forgot-password screens use, with the server's rules kept where they belong.
//!
//! The flow, as on the desktop (`pages/auth_forms.rs`):
//! - `registration_status` says whether sign-ups are open, capped or invite-only; a paused or
//!   invite-only server offers the waitlist instead of the form;
//! - `register` sends the chat key's *public* half along with the account, and answers with the
//!   token plus, for accounts without an email, recovery codes shown once;
//! - `forgot_password` sends a 6-digit code by email, `reset_password` takes it with a new password;
//! - `recover_password` is the way back in with a recovery code when the email is gone.

use crate::api::{auth, client::Api};
use crate::mobile::MobileError;

/// The server's answer (403) when a one-time code is needed: the phone then shows the code field.
#[uniffi::export]
pub fn two_factor_required() -> String {
    auth::TWO_FACTOR_REQUIRED.to_string()
}

/// The Terms/Privacy version the sign-up checkbox accepts (the web's legal.ts TERMS_VERSION).
pub const TERMS_VERSION: &str = "2026-10-02";

#[uniffi::export]
pub fn terms_version() -> String {
    TERMS_VERSION.to_string()
}

/// Sign-ups open? Every field of `RegistrationStatus` as JSON.
#[uniffi::export(async_runtime = "tokio")]
pub async fn registration_status(base: String) -> Result<String, MobileError> {
    let status = auth::registration_status(Api::guest(&base)).await?;
    Ok(serde_json::to_string(&status)?)
}

/// Creates an account. `email` is empty for an access-code sign-up; `e2ee_public_jwk` is this
/// device's chat key, so the account can be reached sealed from the first message.
/// Answers `{"token","user","recovery_codes"}` — the token is what `Client` wants next.
#[uniffi::export(async_runtime = "tokio")]
pub async fn register(
    base: String,
    email: String,
    username: String,
    password: String,
    access_code: String,
    e2ee_public_jwk: String,
) -> Result<String, MobileError> {
    let empty = |s: String| {
        let trimmed = s.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    };
    let form = auth::Registration {
        email: empty(email),
        username: username.trim().to_string(),
        password,
        display_name: None,
        e2ee_public_key: empty(e2ee_public_jwk),
        accepted_terms: Some(TERMS_VERSION.to_string()),
        access_code: empty(access_code),
    };
    let answer = auth::register(Api::guest(&base), form).await?;
    Ok(serde_json::to_string(&serde_json::json!({
        "token": answer.access_token,
        "user": answer.user,
        "recovery_codes": answer.recovery_codes,
    }))?)
}

/// Puts an email on the waitlist when sign-ups are closed.
#[uniffi::export(async_runtime = "tokio")]
pub async fn join_waitlist(base: String, email: String) -> Result<String, MobileError> {
    let status = auth::join_waitlist(Api::guest(&base), email.trim().to_string()).await?;
    Ok(serde_json::to_string(&status)?)
}

/// Sends a 6-digit code to the account's email.
#[uniffi::export(async_runtime = "tokio")]
pub async fn forgot_password(base: String, email: String) -> Result<String, MobileError> {
    let status = auth::forgot_password(Api::guest(&base), email.trim().to_string()).await?;
    Ok(serde_json::to_string(&status)?)
}

/// The new password with the code the email carried.
#[uniffi::export(async_runtime = "tokio")]
pub async fn reset_password(base: String, login: String, code: String, new_password: String) -> Result<String, MobileError> {
    let status = auth::reset_password(Api::guest(&base), login.trim().to_string(), code.trim().to_string(), new_password).await?;
    Ok(serde_json::to_string(&status)?)
}

/// The new password with a recovery code (an account whose email is gone).
#[uniffi::export(async_runtime = "tokio")]
pub async fn recover_password(base: String, login: String, recovery_code: String, new_password: String) -> Result<String, MobileError> {
    let status = auth::recover_password(Api::guest(&base), login.trim().to_string(), recovery_code.trim().to_string(), new_password).await?;
    Ok(serde_json::to_string(&status)?)
}
