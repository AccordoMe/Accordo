// SPDX-License-Identifier: GPL-3.0-or-later
//! GIFs (src/api/gifs.ts): search and trending through the server's proxy.
//!
//! `preview` and `url` are paths on OUR server (/api/gifs/media/<signed token>), never the
//! provider's CDN: only those are ever loaded — a third-party URL in a message would leak the
//! reader's IP.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::client::{enc, Api, ApiResult};

pub const GIF_MEDIA_PATH: &str = "/api/gifs/media/";

/// "klipy" | "giphy"
pub type GifProvider = String;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct GifItem {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub preview_width: i64,
    pub preview_height: i64,
    pub url: String,
    pub width: i64,
    pub height: i64,
    pub provider: GifProvider,
}

/// What a "gif" chat message carries in attachment_data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GifAttachment {
    /// "gif"
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    pub title: Option<String>,
    pub url: String,
    pub preview: String,
    pub width: i64,
    pub height: i64,
    pub provider: GifProvider,
}

impl From<&GifItem> for GifAttachment {
    fn from(g: &GifItem) -> Self {
        Self {
            kind: "gif".into(),
            id: g.id.clone(),
            title: Some(g.title.clone()),
            url: g.url.clone(),
            preview: g.preview.clone(),
            width: g.width,
            height: g.height,
            provider: g.provider.clone(),
        }
    }
}

/// The full URL of a proxied GIF path; None for anything that isn't one.
pub fn gif_src(api: &Api, path: &str) -> Option<String> {
    path.starts_with(GIF_MEDIA_PATH).then(|| api.url(path))
}

#[derive(Debug, Clone, Default)]
pub struct GifStatus {
    pub enabled: bool,
    pub provider: Option<GifProvider>,
}

static STATUS: OnceLock<GifStatus> = OnceLock::new();

/// Which GIF provider the server uses, if any (asked once per run; a failure reads as "off").
pub async fn status(api: Api) -> GifStatus {
    if let Some(s) = STATUS.get() {
        return s.clone();
    }
    let s = match api.get_data::<Value>("/api/gifs/status").await {
        Ok(v) => GifStatus {
            enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(false),
            provider: v.get("provider").and_then(Value::as_str).map(str::to_string),
        },
        Err(_) => GifStatus::default(),
    };
    STATUS.get_or_init(|| s).clone()
}

pub async fn enabled(api: Api) -> bool {
    status(api).await.enabled
}

/// "gifs" or "stickers".
#[derive(Debug, Clone, Copy, Default)]
pub enum Kind {
    #[default]
    Gifs,
    Stickers,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Gifs => "gifs",
            Kind::Stickers => "stickers",
        }
    }
}

fn results(v: Value) -> Vec<GifItem> {
    v.get("results").and_then(|r| serde_json::from_value(r.clone()).ok()).unwrap_or_default()
}

pub async fn search(api: Api, q: String, offset: i64, kind: Kind) -> ApiResult<Vec<GifItem>> {
    let v: Value = api.get_data(&format!("/api/gifs/search?q={}&offset={offset}&kind={}", enc(&q), kind.as_str())).await?;
    Ok(results(v))
}

pub async fn trending(api: Api, offset: i64, kind: Kind) -> ApiResult<Vec<GifItem>> {
    let v: Value = api.get_data(&format!("/api/gifs/trending?offset={offset}&kind={}", kind.as_str())).await?;
    Ok(results(v))
}
