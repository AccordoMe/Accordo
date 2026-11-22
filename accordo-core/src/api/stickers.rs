// SPDX-License-Identifier: GPL-3.0-or-later
//! Stickers (src/api/stickers.ts): Accordo's own packs, and stickers from the GIF provider.
//! Only images served by us are ever loaded (our sticker store or the GIF proxy).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::{Api, ApiResult};
use super::gifs::{GifItem, GIF_MEDIA_PATH};

pub const STICKER_PATH: &str = "/api/stickers/";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AccordoSticker {
    pub id: i64,
    pub pack_id: i64,
    pub emoji: Option<String>,
    pub width: i64,
    pub height: i64,
    /// /api/stickers/<id>/image
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct StickerPack {
    pub id: i64,
    pub name: String,
    pub stickers: Vec<AccordoSticker>,
}

/// What a "sticker" chat message carries in attachment_data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StickerAttachment {
    /// "sticker"
    #[serde(rename = "type")]
    pub kind: String,
    /// accordo | klipy | giphy
    pub source: String,
    pub id: String,
    pub url: String,
    pub width: i64,
    pub height: i64,
    pub title: Option<String>,
}

impl From<&AccordoSticker> for StickerAttachment {
    fn from(s: &AccordoSticker) -> Self {
        Self { kind: "sticker".into(), source: "accordo".into(), id: s.id.to_string(), url: s.url.clone(), width: s.width, height: s.height, title: s.emoji.clone().filter(|e| !e.is_empty()) }
    }
}

impl From<&GifItem> for StickerAttachment {
    fn from(g: &GifItem) -> Self {
        Self { kind: "sticker".into(), source: g.provider.clone(), id: g.id.clone(), url: g.url.clone(), width: g.width, height: g.height, title: Some(g.title.clone()) }
    }
}

/// The full URL of a sticker image; None for paths that aren't ours.
pub fn sticker_src(api: &Api, path: &str) -> Option<String> {
    (path.starts_with(STICKER_PATH) || path.starts_with(GIF_MEDIA_PATH)).then(|| api.url(path))
}

pub async fn packs(api: Api) -> ApiResult<Vec<StickerPack>> {
    api.get_data("/api/stickers/packs").await
}

// Admin only (enforced server-side).

pub async fn create_pack(api: Api, name: String) -> ApiResult<StickerPack> {
    api.post_data("/api/stickers/packs", Some(&json!({ "name": name }))).await
}

pub async fn delete_pack(api: Api, id: i64) -> ApiResult<Value> {
    api.delete_data(&format!("/api/stickers/packs/{id}")).await
}

/// `image`: a data URL of the sticker picture.
pub async fn upload(api: Api, pack_id: i64, image: String, emoji: Option<String>) -> ApiResult<AccordoSticker> {
    let mut body = json!({ "image": image });
    if let Some(e) = emoji {
        body["emoji"] = json!(e);
    }
    api.post_data(&format!("/api/stickers/packs/{pack_id}/stickers"), Some(&body)).await
}

pub async fn delete_sticker(api: Api, id: i64) -> ApiResult<Value> {
    api.delete_data(&format!("/api/stickers/{id}")).await
}
