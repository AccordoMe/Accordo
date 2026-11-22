// SPDX-License-Identifier: GPL-3.0-or-later
//! Accounts (src/api/auth.ts): sign-up / sign-in, profile, two-factor, email, Discord, public
//! profiles. `Session` is the desktop's own: a signed-in user with the server it belongs to.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::{Api, ApiResult};

/// Login answers 403 with this when the password was right and a second factor is needed.
pub const TWO_FACTOR_REQUIRED: &str = "two_factor_required";

/// UserRead.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct User {
    pub id: i64,
    /// None: a username-only account (signed up with an access code).
    pub email: Option<String>,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
    pub discord_user_id: Option<String>,
    pub discord_username: Option<String>,
    pub discord_avatar: Option<String>,
    pub discord_linked_at: Option<String>,
    pub allowed_bots_limit: i64,
    pub current_bot_count: i64,
    pub req_pay_amount: f64,
    pub paid_until: Option<String>,
    pub created_at: Option<String>,
    pub api_key: Option<String>,
    pub api_login: bool,
    pub wallets: Option<Vec<Value>>,
    /// The addresses of `wallets` others see (profile, chats, /pay). None: a server without the
    /// choice — everything shows there, and there's no switch.
    pub public_wallets: Option<Vec<String>>,
    pub data: Option<Value>,
    pub e2ee_public_key: Option<String>,
    /// Terms/Privacy version this user accepted (None = never).
    pub terms_version: Option<String>,
    pub is_admin: bool,
    /// Public chat rooms are admin-only unless the server opens them to everyone.
    pub can_create_public_rooms: bool,
    /// May post articles in chats (granted by admins; admins always can).
    pub can_write_articles: bool,
    /// May use the developer tools (granted by admins; admins always may). GET /api/developer/me.
    pub is_developer: bool,
    /// The profile's label ("member", "team"…), set by admins — no rights of its own.
    pub role: Option<String>,
    /// Organizations this account is in (only in /me).
    pub orgs: Vec<crate::api::orgs::OrgBrief>,
    /// Own rooms this account may show in everyone's Suggested (admins: None = no limit).
    pub suggested_rooms_limit: Option<i64>,
    /// Private groups this account may own (admins: None = no limit).
    pub private_groups_limit: Option<i64>,
    /// Public rooms this account may own (admins: None = no limit).
    pub public_rooms_limit: Option<i64>,
    /// Mini-apps this account may register as a developer (admins: None = no limit).
    pub mini_apps_limit: Option<i64>,
    /// Subscriptions: paid until (None = never / ended). Basic < Lite < Pro.
    pub basic_until: Option<String>,
    pub lite_until: Option<String>,
    pub pro_until: Option<String>,
    pub enterprise_until: Option<String>,
    /// The highest active subscription: "basic" / "lite" / "pro", None = none.
    pub subscription: Option<String>,
    /// Webhooks are on (an active subscription that includes them).
    pub webhooks_active: bool,
    /// Must confirm the email (code or access code) before using the app.
    pub email_verification_required: bool,
    pub session_via_api_key: bool,
    pub verified: Option<bool>,
    /// Two-factor login (authenticator app) is on.
    pub totp_enabled: bool,
    /// Unused one-time recovery codes left.
    pub recovery_codes_left: u32,
    /// An encrypted chat-key backup exists on Accordo: a device without the key should restore it.
    pub has_key_backup: bool,
    /// The mail server reported the email: "bounce" or "complaint". Alerts to it are paused.
    pub email_problem: Option<String>,
    /// Invite codes an admin gave this user to hand out (only in /me).
    pub invites: Option<Vec<InviteItem>>,
}

pub type UserRead = User;

impl User {
    /// The account's picture (its own, else the Discord one).
    pub fn picture(&self) -> Option<&str> {
        self.avatar.as_deref().filter(|s| !s.trim().is_empty()).or(self.discord_avatar.as_deref())
    }

    pub fn name(&self) -> &str {
        self.display_name.as_deref().filter(|n| !n.is_empty()).unwrap_or(&self.username)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub token_type: String,
    pub user: User,
    /// Sign-up without an email only: shown once.
    #[serde(default)]
    pub recovery_codes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RegistrationStatus {
    pub enabled: bool,
    pub max_users: i64,
    pub current_users: i64,
    /// None = no user cap.
    pub remaining_spots: Option<i64>,
    pub invite_code_required: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct InviteItem {
    pub code: String,
    pub used_by: Option<String>,
    pub used_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PublicWallet {
    pub id: Option<String>,
    pub name: Option<String>,
    pub address: String,
    pub chain: Option<String>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PublicUserProfile {
    pub id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
    pub discord_avatar: Option<String>,
    pub discord_username: Option<String>,
    pub discord_user_id: Option<String>,
    pub discord_id: Option<String>,
    pub role: Option<String>,
    pub verified: Option<bool>,
    pub e2ee_public_key: Option<String>,
    pub wallets: Vec<PublicWallet>,
    pub created_at: Option<String>,
}

/// `{status: "..."}` and similar small answers.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Status {
    pub status: String,
    /// forgotPassword: a sentence to show.
    pub detail: Option<String>,
    /// sendEmailVerification / addEmail: where the code went.
    pub email: Option<String>,
    /// recoverPassword: codes left after using one.
    pub recovery_codes_left: Option<u32>,
    /// twoFactorEnable: the new recovery codes (shown once).
    pub recovery_codes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TwoFactorSetup {
    pub secret: String,
    pub otpauth_uri: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecoveryCodes {
    pub recovery_codes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Url {
    pub url: String,
}

/// updateProfile: send only what changes.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProfileUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Some(None) removes the avatar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_login: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallets: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Sign-up fields; `access_code` also goes as the X-Access-Code header (it passes the invite gate).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Registration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub username: String,
    pub password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub e2ee_public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_terms: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_code: Option<String>,
}

// ── sign-up / sign-in ──

pub async fn register(api: Api, mut form: Registration) -> ApiResult<TokenResponse> {
    form.email = form.email.map(|e| e.trim().to_string()).filter(|e| !e.is_empty());
    form.access_code = form.access_code.filter(|c| !c.is_empty());
    let headers: Vec<(&str, &str)> = form.access_code.as_deref().map(|c| vec![("X-Access-Code", c)]).unwrap_or_default();
    let (status, value) = api.request_value(reqwest::Method::POST, "/api/auth/register", Some(&form), &headers).await?;
    serde_json::from_value(value).map_err(|e| super::client::ApiError { status: Some(status), message: format!("Unexpected answer from the server: {e}") })
}

pub async fn accept_terms(api: Api, version: String) -> ApiResult<User> {
    api.post("/api/auth/accept-terms", &json!({ "version": version })).await
}

pub async fn login(api: Api, email_or_username: String, password: String, otp: Option<String>) -> ApiResult<TokenResponse> {
    let mut body = json!({ "email_or_username": email_or_username.trim(), "password": password });
    if let Some(otp) = otp.filter(|o| !o.is_empty()) {
        body["otp"] = json!(otp);
    }
    api.post("/api/auth/login", &body).await
}

pub async fn login_with_api_key(api: Api, api_key: String) -> ApiResult<TokenResponse> {
    api.post("/api/auth/login/api-key", &json!({ "api_key": api_key })).await
}

pub async fn registration_status(api: Api) -> ApiResult<RegistrationStatus> {
    api.get("/api/auth/registration-status").await
}

pub async fn join_waitlist(api: Api, email: String) -> ApiResult<Status> {
    api.post("/api/auth/waitlist", &json!({ "email": email })).await
}

// ── two-factor, recovery ──

pub async fn two_factor_setup(api: Api, password: String) -> ApiResult<TwoFactorSetup> {
    api.post("/api/auth/2fa/setup", &json!({ "password": password })).await
}

pub async fn two_factor_enable(api: Api, code: String) -> ApiResult<Status> {
    api.post("/api/auth/2fa/enable", &json!({ "code": code })).await
}

pub async fn two_factor_disable(api: Api, password: String, code: String) -> ApiResult<Status> {
    api.post("/api/auth/2fa/disable", &json!({ "password": password, "code": code })).await
}

pub async fn regenerate_recovery_codes(api: Api, password: String, code: Option<String>) -> ApiResult<RecoveryCodes> {
    let mut body = json!({ "password": password });
    if let Some(code) = code.filter(|c| !c.is_empty()) {
        body["code"] = json!(code);
    }
    api.post("/api/auth/recovery-codes", &body).await
}

pub async fn recover_password(api: Api, login: String, recovery_code: String, new_password: String) -> ApiResult<Status> {
    api.post("/api/auth/password/recover", &json!({ "login": login, "recovery_code": recovery_code, "new_password": new_password })).await
}

// ── profile ──

pub async fn me(api: Api) -> ApiResult<User> {
    api.get("/api/auth/me").await
}

/// One of the account's limits, for its profile card: the limit from /me (None = none, admins),
/// `used` counted from the chat list here (None = not known here: shown as "up to N").
#[derive(Debug, Clone, Default)]
pub struct AccountLimit {
    pub label: &'static str,
    pub used: Option<i64>,
    pub limit: Option<i64>,
}

impl User {
    /// The limits /me gives, with what this account uses of each — counted from its rooms (the
    /// same rule as the server's checks: rooms it created; Suggested = those with a view).
    pub fn limits(&self, rooms: &[crate::api::chat::ChatRoomItem]) -> Vec<AccountLimit> {
        let own = |kind: &str| rooms.iter().filter(|r| r.created_by == Some(self.id) && r.chat_type == kind).count() as i64;
        let shown = rooms.iter().filter(|r| r.created_by == Some(self.id) && r.view.as_ref().is_some_and(|v| !v.is_empty())).count() as i64;
        // As the server sends them: None = no limit (admins by default, or the role's file section).
        vec![
            AccountLimit { label: "Groups", used: Some(own("group")), limit: self.private_groups_limit },
            AccountLimit { label: "Public rooms", used: Some(own("public")), limit: self.public_rooms_limit },
            AccountLimit { label: "In Suggested", used: Some(shown), limit: self.suggested_rooms_limit },
            AccountLimit { label: "Mini-apps", used: None, limit: self.mini_apps_limit },
        ]
    }
}

pub async fn update_profile(api: Api, update: ProfileUpdate) -> ApiResult<User> {
    api.put("/api/auth/profile", &update).await
}

pub async fn patch_data(api: Api, data: Value) -> ApiResult<User> {
    api.patch("/api/auth/data", &json!({ "data": data })).await
}

pub async fn update_wallets(api: Api, wallets: Vec<Value>) -> ApiResult<User> {
    api.put("/api/auth/wallets", &json!({ "wallets": wallets })).await
}

/// Which wallets others see — the whole list (empty hides all). Apart from `update_wallets`, so
/// rewriting the wallets never touches it.
pub async fn set_public_wallets(api: Api, addresses: Vec<String>) -> ApiResult<User> {
    api.put("/api/auth/wallets/public", &json!({ "addresses": addresses })).await
}

/// Is this wallet among the shown ones? EVM addresses match in any case, base58 ones exactly
/// (the server's wallet_key).
pub fn is_public(public: &[String], chain: Option<&str>, address: &str) -> bool {
    let evm = chain.is_none_or(|c| c.eq_ignore_ascii_case("EVM"));
    public.iter().any(|p| if evm { p.eq_ignore_ascii_case(address) } else { p == address })
}

pub async fn regenerate_api_key(api: Api) -> ApiResult<User> {
    api.post_empty("/api/auth/regenerate-api-key").await
}

// ── password reset, email ──

pub async fn forgot_password(api: Api, email: String) -> ApiResult<Status> {
    api.post("/api/auth/password/forgot", &json!({ "email": email })).await
}

pub async fn reset_password(api: Api, email: String, code: String, new_password: String) -> ApiResult<Status> {
    api.post("/api/auth/password/reset", &json!({ "email": email, "code": code, "new_password": new_password })).await
}

pub async fn send_email_verification(api: Api) -> ApiResult<Status> {
    api.post_empty("/api/auth/email/send-verification").await
}

pub async fn verify_with_access_code(api: Api, code: String) -> ApiResult<Status> {
    api.post("/api/auth/email/verify-access-code", &json!({ "code": code })).await
}

pub async fn verify_email(api: Api, code: String) -> ApiResult<Status> {
    api.post("/api/auth/email/verify", &json!({ "code": code })).await
}

/// Username-only accounts: sends a code to a new address; it's saved once the code is confirmed.
pub async fn add_email(api: Api, email: String) -> ApiResult<Status> {
    api.post("/api/auth/email/add", &json!({ "email": email })).await
}

pub async fn confirm_add_email(api: Api, email: String, code: String) -> ApiResult<Status> {
    api.post("/api/auth/email/add/confirm", &json!({ "email": email, "code": code })).await
}

// ── Discord ──

pub async fn discord_login_url(api: Api) -> ApiResult<Url> {
    api.get("/api/auth/discord/login-url").await
}

pub async fn discord_callback(api: Api, code: String, state: String) -> ApiResult<User> {
    api.post("/api/auth/discord/callback", &json!({ "code": code, "state": state })).await
}

pub async fn discord_unlink(api: Api) -> ApiResult<User> {
    api.post_empty("/api/auth/discord/unlink").await
}

pub async fn discord_data(api: Api) -> ApiResult<Value> {
    api.get("/api/auth/discord/data").await
}

// ── public profiles & wallets (no vault data) ──

/// `user_ref`: a username or a numeric id.
pub async fn get_user_profile(api: Api, user_ref: String) -> ApiResult<PublicUserProfile> {
    api.get(&format!("/api/users/{user_ref}")).await
}

pub async fn get_user_wallets(api: Api, user_ref: String) -> ApiResult<Vec<PublicWallet>> {
    api.get(&format!("/api/users/{user_ref}/wallets")).await
}

pub async fn get_user_by_address(api: Api, address: String) -> ApiResult<PublicUserProfile> {
    api.get(&format!("/api/users/by-address/{address}")).await
}

// ── the desktop's session ──

/// A signed-in user, with the server it belongs to (remembered by session_store).
#[derive(Debug, Clone)]
pub struct Session {
    pub base: String,
    pub token: String,
    pub user: User,
}

impl Session {
    pub fn api(&self) -> Api {
        Api::new(&self.base, Some(&self.token))
    }

    pub fn saved(&self) -> super::session_store::Saved {
        super::session_store::Saved { base: self.base.clone(), token: self.token.clone(), moved_from: None }
    }

    /// Which account on which server: "1:dazay@https://accordo.me" (the credential store's
    /// entries, the switcher, the local data).
    pub fn key(&self) -> String {
        super::session_store::account_key(self.user.id, &self.user.username, &self.base)
    }

    /// The key's earlier form (id@server): what to look under for entries not moved yet.
    pub fn previous_key(&self) -> String {
        super::session_store::previous_account_key(self.user.id, &self.base)
    }
}

/// Sign in and keep the session.
pub async fn sign_in(base: String, email_or_username: String, password: String, otp: Option<String>) -> ApiResult<Session> {
    let res = login(Api::guest(&base), email_or_username, password, otp).await?;
    Ok(Session { base, token: res.access_token, user: res.user })
}

/// Checks a saved token: the user it belongs to, or 401 when it expired / was revoked.
pub async fn restore(base: String, token: String) -> ApiResult<Session> {
    let user = me(Api::new(&base, Some(&token))).await?;
    Ok(Session { base, token, user })
}



#[cfg(test)]
mod public_wallet_tests {
    use super::is_public;

    #[test]
    fn evm_matches_in_any_case_base58_exactly() {
        let public = vec!["0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266".to_string(), "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM".to_string()];
        assert!(is_public(&public, None, "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"));
        assert!(is_public(&public, Some("evm"), "0xF39FD6E51AAD88F6F4CE6AB8827279CFFFB92266"));
        assert!(is_public(&public, Some("Solana"), "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"));
        assert!(!is_public(&public, Some("Solana"), "9wzdxwbbmkg8ztbnmquxvqrayrzzdsgydlvl9zytawwm"));
        assert!(!is_public(&[], None, "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"));
    }
}
