// SPDX-License-Identifier: GPL-3.0-or-later
//! HTTP client (src/api/client.ts): base URL, sign-in token, device id, JSON requests, the
//! `{data: …}` envelope some endpoints use, file download / upload, and retries for on-chain txs.
//!
//! Every endpoint function in `api::*` takes an [`Api`] by value: it's two shared strings, cheap to
//! clone, and owning it makes the returned future `'static`, as `iced::Task::perform` needs.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use reqwest::{Method, RequestBuilder};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

/// The public Accordo server: API and web app share the domain.
pub const DEFAULT_API_URL: &str = "https://accordo.me";

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(user_agent())
        .timeout(Duration::from_secs(30))
        .use_preconfigured_tls(tls_config())
        .build()
        .expect("HTTP client")
});

/// TLS with the bundled Mozilla roots. reqwest's own `rustls` feature asks the *platform* for
/// certificates, which on Android needs the rustls-platform-verifier JNI glue; with our own root
/// store the same bytes work on every target and no JVM call is involved.
fn tls_config() -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    rustls::ClientConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
        .with_safe_default_protocol_versions()
        .expect("safe TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth()
}

/// "AccordoDesktop/0.1.0 (Linux)": the server names sessions from the user agent
/// ("… on Linux"), so the system goes in like a browser's would.
fn user_agent() -> String {
    let system = match std::env::consts::OS {
        "android" => "Android",
        "linux" => "Linux",
        "windows" => "Windows",
        "macos" => "Mac OS X",
        other => other,
    };
    format!("AccordoDesktop/{} ({system})", env!("CARGO_PKG_VERSION"))
}

/// An error to show the user: the server's `detail`, or what went wrong on the way.
#[derive(Debug, Clone)]
pub struct ApiError {
    /// HTTP status; None when the server wasn't reached.
    pub status: Option<u16>,
    pub message: String,
}

impl ApiError {
    fn decode(status: u16, err: impl std::fmt::Display) -> Self {
        Self { status: Some(status), message: format!("Unexpected answer from the server: {err}") }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

pub type ApiResult<T> = Result<T, ApiError>;

/// `accordo.me` → `https://accordo.me`, `127.0.0.1:8000` → `http://127.0.0.1:8000`
/// (normalizeApiBase in client.ts: local addresses get http, everything else https).
pub fn normalize_base(raw: &str) -> String {
    let s = raw.trim().trim_end_matches('/');
    if s.is_empty() || s.starts_with("http://") || s.starts_with("https://") {
        return s.to_string();
    }
    let host = s.split('/').next().unwrap_or("").split(':').next().unwrap_or("").to_lowercase();
    let private_172 = host
        .strip_prefix("172.")
        .and_then(|rest| rest.split('.').next())
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (16..=31).contains(&n));
    let local = host == "localhost"
        || host.ends_with(".localhost")
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || private_172;
    format!("{}://{s}", if local { "http" } else { "https" })
}

/// encodeURIComponent: everything but `A-Z a-z 0-9 - _ . ! ~ * ' ( )` is percent-encoded.
pub fn enc(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `?a=1&b=x` from the pairs that have a value (URLSearchParams with the empty ones left out);
/// "" when none do.
pub fn query(pairs: &[(&str, Option<String>)]) -> String {
    let parts: Vec<String> = pairs.iter().filter_map(|(k, v)| v.as_ref().map(|v| format!("{}={}", enc(k), enc(v)))).collect();
    if parts.is_empty() { String::new() } else { format!("?{}", parts.join("&")) }
}

/// A random id for this computer, so the server can tell a sign-in from a new device and email
/// about it (X-Accordo-Device, like the browser's `accordo.device`). Kept in the user's config
/// directory; if that can't be written, requests go without it and the server uses the user agent.
static DEVICE_ID: LazyLock<Option<String>> = LazyLock::new(|| {
    let dir = config_dir()?;
    let file = dir.join("device-id");
    if let Ok(id) = std::fs::read_to_string(&file) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return Some(id);
        }
    }
    let id = uuid_v4()?;
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(&file, &id).ok()?;
    Some(id)
});

/// Linux: $XDG_CONFIG_HOME/accordo-desktop (~/.config/…); Windows: %APPDATA%\Accordo;
/// macOS: ~/Library/Application Support/Accordo.
/// `ACCORDO_PROFILE=<name>`: a separate set of settings and accounts — e.g. two windows signed in
/// as two users side by side for testing (scripts/dev-two.sh). Letters, digits, `-` and `_`, up to
/// 32; anything else is ignored. The OS secret store is shared: its entries are per account anyway.
pub fn profile() -> Option<&'static str> {
    static PROFILE: LazyLock<Option<String>> = LazyLock::new(|| {
        let p = std::env::var("ACCORDO_PROFILE").ok()?.trim().to_lowercase();
        (!p.is_empty() && p.len() <= 32 && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then_some(p)
    });
    PROFILE.as_deref()
}

/// A directory the host chose (a phone's files dir): used instead of the env-var guess below.
static CHOSEN_CONFIG_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// The host's own directory for settings (Android has no XDG/HOME to fall back on).
pub fn set_config_dir(path: std::path::PathBuf) {
    let _ = CHOSEN_CONFIG_DIR.set(path);
}

pub fn config_dir() -> Option<std::path::PathBuf> {
    CHOSEN_CONFIG_DIR.get().cloned().or_else(|| config_dir_of(profile()))
}

/// The settings of the usual profile (no ACCORDO_PROFILE), whichever this one is.
pub fn default_config_dir() -> Option<std::path::PathBuf> {
    config_dir_of(None)
}

fn config_dir_of(profile: Option<&str>) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let name = |base: &str| match profile {
        Some(p) => format!("{base}-{p}"),
        None => base.to_string(),
    };
    if cfg!(windows) {
        env("APPDATA").map(|p| p.join(name("Accordo")))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|p| p.join("Library/Application Support").join(name("Accordo")))
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config"))).map(|p| p.join(name("accordo-desktop")))
    }
}

pub fn uuid_v4() -> Option<String> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).ok()?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Some(format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32]))
}

/// The `{data: …}` envelope: some endpoints wrap their answer, the web unwraps it with
/// `res.data || res` — the same rule here (a missing / null / false `data` means "not wrapped").
pub fn unwrap_data(value: Value) -> Value {
    match value {
        Value::Object(mut map) if map.get("data").is_some_and(|d| !matches!(d, Value::Null | Value::Bool(false))) => {
            map.remove("data").unwrap_or(Value::Null)
        }
        other => other,
    }
}

/// A list answer that may come bare or as `{data: [...]}`; anything else is an empty list
/// (chat.ts getRooms / getMessages, queries.ts asList).
pub fn as_list(value: Value) -> Value {
    match value {
        Value::Array(_) => value,
        Value::Object(mut map) => match map.remove("data") {
            Some(list @ Value::Array(_)) => list,
            _ => Value::Array(Vec::new()),
        },
        _ => Value::Array(Vec::new()),
    }
}

fn from_value<T: DeserializeOwned>(value: Value, status: u16) -> ApiResult<T> {
    serde_json::from_value(value).map_err(|e| ApiError::decode(status, e))
}

/// The server a signed-in (or guest) user talks to.
#[derive(Debug, Clone)]
pub struct Api {
    base: Arc<str>,
    token: Option<Arc<str>>,
}

impl Api {
    pub fn new(base: &str, token: Option<&str>) -> Self {
        Self { base: normalize_base(base).into(), token: token.filter(|t| !t.is_empty()).map(Into::into) }
    }

    /// Without a token (public endpoints, sign-in).
    pub fn guest(base: &str) -> Self {
        Self::new(base, None)
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// The bearer token, if signed in (the socket needs it).
    pub fn token(&self) -> Option<String> {
        self.token.as_ref().map(|t| t.to_string())
    }

    pub fn with_token(&self, token: &str) -> Self {
        Self { base: self.base.clone(), token: Some(token.into()) }
    }

    /// `<base><path>`, for links to files the server serves (stickers, GIF proxy).
    pub fn url(&self, path: &str) -> String {
        let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
        format!("{}{path}", self.base)
    }

    fn builder(&self, method: Method, path: &str) -> RequestBuilder {
        let mut request = HTTP.request(method, self.url(path));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(device) = DEVICE_ID.as_deref() {
            request = request.header("X-Accordo-Device", device);
        }
        request
    }

    /// Sends and returns (status, body). Errors carry FastAPI's `detail` like the web client.
    /// Each request is recorded for the developer tools (`trace`: method, path, request body).
    async fn send(&self, request: RequestBuilder, trace: Trace) -> ApiResult<(u16, Vec<u8>)> {
        let started = std::time::Instant::now();
        let result = self.exchange(request).await;
        if crate::log::enabled() {
            let (status, bytes, error) = match &result {
                Ok((code, bytes)) => (Some(*code), Some(bytes.as_slice()), None),
                Err((e, bytes)) => (e.status, bytes.as_deref(), Some(e.message.clone())),
            };
            crate::log::record_net(crate::log::Net {
                method: trace.method,
                url: self.url(&trace.path),
                path: trace.path,
                status,
                millis: started.elapsed().as_millis(),
                size: bytes.map_or(0, <[u8]>::len),
                request_body: trace.body,
                response_body: bytes.and_then(crate::log::body_text),
                error,
            });
        }
        result.map_err(|(e, _)| e)
    }

    async fn exchange(&self, request: RequestBuilder) -> Result<(u16, Vec<u8>), (ApiError, Option<Vec<u8>>)> {
        let response = request.send().await.map_err(|e| {
            let message = if e.is_timeout() {
                format!("{} didn't answer in time", self.base)
            } else {
                format!("Can't reach {}. Check the address and your connection.", self.base)
            };
            tracing::warn!("{message} ({e})");
            (ApiError { status: None, message }, None)
        })?;
        let status = response.status();
        let code = status.as_u16();
        let bytes = response.bytes().await.map_err(|e| (ApiError { status: Some(code), message: format!("The answer was cut off: {e}") }, None))?.to_vec();
        if status.is_success() {
            return Ok((code, bytes));
        }
        // FastAPI errors: {"detail": "..."} (or a list / object: sent on as JSON text).
        let detail = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .map(|v| match v {
                Value::Object(mut m) => m.remove("detail").unwrap_or(Value::Object(m)),
                other => other,
            })
            .map(|d| match d {
                Value::String(s) => s,
                other => other.to_string(),
            })
            .unwrap_or_else(|| status.canonical_reason().unwrap_or("Request failed").to_string());
        Err((ApiError { status: Some(code), message: detail }, Some(bytes)))
    }

    /// JSON request → raw JSON answer (`null` for 204 / an empty body).
    pub async fn request_value<B: Serialize + ?Sized>(&self, method: Method, path: &str, body: Option<&B>, headers: &[(&str, &str)]) -> ApiResult<(u16, Value)> {
        let trace = Trace {
            method: method.to_string(),
            path: path.to_string(),
            body: body.filter(|_| crate::log::enabled()).and_then(|b| serde_json::to_value(b).ok()).map(|v| crate::log::json_text(&v)),
        };
        let mut request = self.builder(method, path);
        if let Some(body) = body {
            request = request.json(body);
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let (status, body) = self.send(request, trace).await?;
        if body.is_empty() {
            return Ok((status, Value::Null));
        }
        let value = serde_json::from_slice(&body).map_err(|e| ApiError::decode(status, e))?;
        Ok((status, value))
    }

    async fn call<B: Serialize + ?Sized, R: DeserializeOwned>(&self, method: Method, path: &str, body: Option<&B>, unwrap: Unwrap) -> ApiResult<R> {
        let (status, value) = self.request_value(method, path, body, &[]).await?;
        let value = match unwrap {
            Unwrap::None => value,
            Unwrap::Data => unwrap_data(value),
            Unwrap::List => as_list(value),
        };
        from_value(value, status)
    }

    // ── plain answers ──

    pub async fn get<R: DeserializeOwned>(&self, path: &str) -> ApiResult<R> {
        self.call::<(), R>(Method::GET, path, None, Unwrap::None).await
    }

    pub async fn post<B: Serialize + ?Sized, R: DeserializeOwned>(&self, path: &str, body: &B) -> ApiResult<R> {
        self.call(Method::POST, path, Some(body), Unwrap::None).await
    }

    /// POST without a body.
    pub async fn post_empty<R: DeserializeOwned>(&self, path: &str) -> ApiResult<R> {
        self.call::<(), R>(Method::POST, path, None, Unwrap::None).await
    }

    pub async fn put<B: Serialize + ?Sized, R: DeserializeOwned>(&self, path: &str, body: &B) -> ApiResult<R> {
        self.call(Method::PUT, path, Some(body), Unwrap::None).await
    }

    pub async fn patch<B: Serialize + ?Sized, R: DeserializeOwned>(&self, path: &str, body: &B) -> ApiResult<R> {
        self.call(Method::PATCH, path, Some(body), Unwrap::None).await
    }

    pub async fn delete<R: DeserializeOwned>(&self, path: &str) -> ApiResult<R> {
        self.call::<(), R>(Method::DELETE, path, None, Unwrap::None).await
    }

    // ── `{data: …}`-wrapped answers (the web's `res.data || res`) ──

    pub async fn get_data<R: DeserializeOwned>(&self, path: &str) -> ApiResult<R> {
        self.call::<(), R>(Method::GET, path, None, Unwrap::Data).await
    }

    pub async fn post_data<B: Serialize + ?Sized, R: DeserializeOwned>(&self, path: &str, body: Option<&B>) -> ApiResult<R> {
        self.call(Method::POST, path, body, Unwrap::Data).await
    }

    pub async fn put_data<B: Serialize + ?Sized, R: DeserializeOwned>(&self, path: &str, body: &B) -> ApiResult<R> {
        self.call(Method::PUT, path, Some(body), Unwrap::Data).await
    }

    pub async fn patch_data<B: Serialize + ?Sized, R: DeserializeOwned>(&self, path: &str, body: &B) -> ApiResult<R> {
        self.call(Method::PATCH, path, Some(body), Unwrap::Data).await
    }

    pub async fn delete_data<R: DeserializeOwned>(&self, path: &str) -> ApiResult<R> {
        self.call::<(), R>(Method::DELETE, path, None, Unwrap::Data).await
    }

    /// A list that may come bare or wrapped; anything else reads as empty.
    pub async fn get_list<R: DeserializeOwned>(&self, path: &str) -> ApiResult<Vec<R>> {
        self.call::<(), Vec<R>>(Method::GET, path, None, Unwrap::List).await
    }

    // ── files ──

    /// A file from an endpoint that needs the sign-in token (api.getBlob).
    pub async fn get_bytes(&self, path: &str) -> ApiResult<Vec<u8>> {
        let (_, body) = self.send(self.builder(Method::GET, path), Trace::new("GET", path, None)).await?;
        Ok(body)
    }

    /// multipart/form-data POST (file uploads, api.postForm); the answer `{data: …}`-unwrapped.
    pub async fn post_form<R: DeserializeOwned>(&self, path: &str, form: reqwest::multipart::Form) -> ApiResult<R> {
        let (status, body) = self.send(self.builder(Method::POST, path).multipart(form), Trace::new("POST", path, Some("<multipart form: files>".into()))).await?;
        let value = if body.is_empty() { Value::Null } else { serde_json::from_slice(&body).map_err(|e| ApiError::decode(status, e))? };
        from_value(unwrap_data(value), status)
    }

    /// Reports a transaction the user already sent on-chain (recordOnChainTx). The backend checks it
    /// against the chain first, and its RPC can be a few seconds behind the wallet's — so "not found
    /// yet", 502/503+ and network errors are retried (5 tries, 3 s apart). Answer `{data: …}`-unwrapped.
    pub async fn record_on_chain_tx<B: Serialize + ?Sized, R: DeserializeOwned>(&self, path: &str, body: &B) -> ApiResult<R> {
        const ATTEMPTS: u32 = 5;
        let mut attempt = 1;
        loop {
            match self.request_value(Method::POST, path, Some(body), &[]).await {
                Ok((status, value)) => return from_value(unwrap_data(value), status),
                Err(err) => {
                    let retryable = match err.status {
                        None => true,
                        Some(400) => err.message.to_lowercase().contains("not found on-chain yet"),
                        Some(s) => s == 502 || s >= 503,
                    };
                    if !retryable || attempt >= ATTEMPTS {
                        return Err(err);
                    }
                    attempt += 1;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }
}

/// What the developer tools record about a request (never its headers: the token is there).
struct Trace {
    method: String,
    path: String,
    body: Option<String>,
}

impl Trace {
    fn new(method: &str, path: &str, body: Option<String>) -> Self {
        Self { method: method.into(), path: path.into(), body }
    }
}

#[derive(Clone, Copy)]
enum Unwrap {
    None,
    Data,
    List,
}

/// A file to upload (NFT covers and collection images).
#[derive(Debug, Clone)]
pub struct UploadFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

impl UploadFile {
    pub(crate) fn part(&self) -> reqwest::multipart::Part {
        let mime = mime_guess::from_path(&self.name).first_or_octet_stream();
        reqwest::multipart::Part::bytes(self.bytes.clone())
            .file_name(self.name.clone())
            .mime_str(mime.as_ref())
            .unwrap_or_else(|_| reqwest::multipart::Part::bytes(self.bytes.clone()).file_name(self.name.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_urls_like_the_web_client() {
        assert_eq!(normalize_base("accordo.me"), "https://accordo.me");
        assert_eq!(normalize_base("https://accordo.me/"), "https://accordo.me");
        assert_eq!(normalize_base("127.0.0.1:8000"), "http://127.0.0.1:8000");
        assert_eq!(normalize_base("localhost:8000"), "http://localhost:8000");
        assert_eq!(normalize_base("192.168.1.5:8000"), "http://192.168.1.5:8000");
        assert_eq!(normalize_base("172.20.0.1"), "http://172.20.0.1");
        assert_eq!(normalize_base("172.40.0.1"), "https://172.40.0.1");
    }

    #[test]
    fn encodes_like_encode_uri_component() {
        assert_eq!(enc("a b/c?d=é"), "a%20b%2Fc%3Fd%3D%C3%A9");
        assert_eq!(enc("Az09-_.!~*'()"), "Az09-_.!~*'()");
        assert_eq!(query(&[("a", Some("1".into())), ("b", None), ("c", Some("x y".into()))]), "?a=1&c=x%20y");
        assert_eq!(query(&[("a", None)]), "");
    }

    #[test]
    fn data_envelope_like_the_web() {
        use serde_json::json;
        assert_eq!(unwrap_data(json!({"data": {"x": 1}})), json!({"x": 1}));
        assert_eq!(unwrap_data(json!({"data": null, "x": 1})), json!({"data": null, "x": 1}));
        assert_eq!(unwrap_data(json!([1, 2])), json!([1, 2]));
        assert_eq!(as_list(json!({"data": [1]})), json!([1]));
        assert_eq!(as_list(json!({"x": 1})), json!([]));
    }

    #[test]
    fn uuid_shape() {
        let id = uuid_v4().unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
    }
}

/// Against the real server, public endpoints only (no account): `cargo test -- --ignored live`.
#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn live_public_endpoints() {
        let base = std::env::var("ACCORDO_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.into());
        let api = Api::guest(&base);
        let chains = crate::api::data::chains(api.clone()).await.expect("chains");
        assert!(!chains.is_empty());
        let (_, tokens) = crate::api::data::tokens(api.clone(), "8453".into()).await.expect("tokens");
        assert!(!tokens.is_empty());
        let reg = crate::api::auth::registration_status(api.clone()).await.expect("registration status");
        println!("chains {}, Base tokens {}, registration enabled {}", chains.len(), tokens.len(), reg.enabled);
        // A protected endpoint without a token: FastAPI's detail comes through as the message.
        let err = crate::api::auth::me(api.clone()).await.expect_err("me needs a token");
        assert_eq!(err.status, Some(401));
        println!("401 detail: {}", err.message);
        let catalog = crate::api::chat::public_rooms::catalog(api, String::new(), 0, 5).await;
        println!("public catalog without a token: {:?}", catalog.map(|c| c.total).map_err(|e| e.status));
    }
}
