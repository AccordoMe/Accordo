// SPDX-License-Identifier: GPL-3.0-or-later
//! Avatar pictures, for every screen (rooms, people, the account): the web's `<img src={avatar}>`.
//!
//! An avatar is a `data:image/…` URL (what the web's resizeImageToDataUrl makes — 160 px), a
//! server path, or a link (a Discord avatar). A view asks with `get`: the picture if it's decoded,
//! otherwise None — the initials show — and it's noted as wanted. The app then loads what's
//! wanted (`pump`, after each update and on a short tick while something waits) off the UI thread.

use crate::app::{DataMsg};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{LazyLock, Mutex};

use iced::widget::image::Handle;
use iced::Task;

use crate::api::client::Api;
use crate::app::Message;

/// Decoded at most this big: a 160 px source, crisp at 84 px on a 2× screen.
const PX: u32 = 192;
/// Links bigger than this aren't loaded.
const MAX_BYTES: usize = 3 * 1024 * 1024;

/// A decoded avatar: a picture, or a drawing (the web's dicebear presets are SVG).
#[derive(Debug, Clone)]
pub enum Pic {
    Raster(Handle),
    Vector(iced::widget::svg::Handle),
}

enum Slot {
    Loading,
    Ready(Pic),
    Failed,
}

#[derive(Default)]
struct Store {
    slots: HashMap<u64, Slot>,
    wanted: HashMap<u64, String>,
}

static STORE: LazyLock<Mutex<Store>> = LazyLock::new(Default::default);

fn id(src: &str) -> u64 {
    let mut h = DefaultHasher::new();
    src.hash(&mut h);
    h.finish()
}

fn usable(src: &str) -> bool {
    src.starts_with("data:image/") || src.starts_with("https://") || src.starts_with("http://") || src.starts_with('/')
}

/// The picture for `src`, if it's ready (otherwise it's asked for, and None: show the initials).
pub fn get(src: Option<&str>) -> Option<Pic> {
    let src = src.map(str::trim).filter(|s| usable(s))?;
    let k = id(src);
    let mut s = STORE.lock().ok()?;
    match s.slots.get(&k) {
        Some(Slot::Ready(h)) => Some(h.clone()),
        Some(_) => None,
        None => {
            s.wanted.entry(k).or_insert_with(|| src.to_string());
            None
        }
    }
}

pub fn has_wanted() -> bool {
    STORE.lock().is_ok_and(|s| !s.wanted.is_empty())
}

/// Starts loading what views asked for.
pub fn pump(api: Option<&Api>) -> Task<Message> {
    let Ok(mut s) = STORE.lock() else { return Task::none() };
    if s.wanted.is_empty() {
        return Task::none();
    }
    let wanted: Vec<(u64, String)> = s.wanted.drain().collect();
    let mut jobs = Vec::new();
    for (k, src) in wanted {
        s.slots.insert(k, Slot::Loading);
        let api = api.cloned();
        jobs.push(Task::perform(load(api, src), move |h| Message::Data(DataMsg::AvatarLoaded(k, h))));
    }
    Task::batch(jobs)
}

pub fn finish(k: u64, h: Option<Pic>) {
    if let Ok(mut s) = STORE.lock() {
        s.slots.insert(k, h.map_or(Slot::Failed, Slot::Ready));
    }
}

async fn load(api: Option<Api>, src: String) -> Option<Pic> {
    let bytes = if src.starts_with("data:") {
        crate::media::parse_data_url(&src)?.1
    } else if src.starts_with('/') {
        api?.get_bytes(&src).await.ok()?
    } else {
        let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build().ok()?;
        let r = client.get(&src).send().await.ok()?.error_for_status().ok()?;
        if r.content_length().is_some_and(|n| n as usize > MAX_BYTES) {
            return None;
        }
        let b = r.bytes().await.ok()?;
        (b.len() <= MAX_BYTES).then(|| b.to_vec())?
    };
    if is_svg(&bytes) {
        return Some(Pic::Vector(iced::widget::svg::Handle::from_memory(bytes)));
    }
    let frames = crate::media::decode_async(bytes, PX, true).await.ok()?;
    frames.frames.first().map(|(h, _)| Pic::Raster(h.clone()))
}

fn is_svg(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_lowercase();
    head.contains("<svg")
}

/// A picture file → the data URL the server keeps (resizeImageToDataUrl: the longer side at
/// `max` px, JPEG at 0.85 — as the web's canvas.toDataURL('image/jpeg', 0.85)).
pub fn file_to_data_url(path: &std::path::Path, max: u32) -> Result<String, String> {
    let img = image::open(path).map_err(|_| "That isn't a picture this app can read".to_string())?;
    let img = if img.width().max(img.height()) > max { img.thumbnail(max, max) } else { img };
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(&img.to_rgb8()).map_err(|e| e.to_string())?;
    Ok(crate::media::data_url("image/jpeg", &out))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_picture_becomes_a_small_jpeg() {
        let dir = std::env::temp_dir().join(format!("accordo-av-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.png");
        image::RgbaImage::from_pixel(400, 200, image::Rgba([10, 200, 30, 128])).save(&p).unwrap();
        let url = super::file_to_data_url(&p, 160).unwrap();
        let (mime, bytes) = crate::media::parse_data_url(&url).unwrap();
        assert_eq!(mime, "image/jpeg");
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!((back.width(), back.height()), (160, 80));
        let _ = std::fs::remove_dir_all(dir);
        assert!(super::is_svg(b"<?xml version=\"1.0\"?><svg xmlns=..."));
        assert!(!super::is_svg(&[0x89, b'P', b'N', b'G']));
    }
}
