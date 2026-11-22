// SPDX-License-Identifier: GPL-3.0-or-later
//! Pictures in the chat: decoding (PNG, JPEG, WebP, GIF — animated ones frame by frame), a small
//! cache, attachments as data URLs (how the web sends files: `data:<mime>;base64,…` inside the
//! message's attachment_data, encrypted with the rest in private rooms), and saving to Downloads.

use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine;
use iced::widget::image::Handle;
use image::{AnimationDecoder, DynamicImage, ImageFormat};

/// Files bigger than this aren't sent (the web's limit).
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// Frames kept of an animation (long GIFs are cut).
const MAX_FRAMES: usize = 120;
/// Pictures kept decoded; the oldest go first.
const CACHE_SIZE: usize = 80;

#[derive(Debug)]
pub struct Frames {
    /// (frame, how long it shows, ms). One frame: a still picture.
    pub frames: Vec<(Handle, u32)>,
    pub width: u32,
    pub height: u32,
    total_ms: u32,
}

impl Frames {
    pub fn animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// The frame to show `clock_ms` into the animation.
    pub fn at(&self, clock_ms: u64) -> &Handle {
        if self.frames.len() <= 1 || self.total_ms == 0 {
            return &self.frames[0].0;
        }
        let mut t = (clock_ms % self.total_ms as u64) as u32;
        for (h, d) in &self.frames {
            if t < *d {
                return h;
            }
            t -= d;
        }
        &self.frames[0].0
    }
}

#[derive(Debug, Clone)]
pub enum Media {
    Loading,
    Ready(Arc<Frames>),
    Failed,
}

/// Decoded pictures by key ("msg:<id>" for attachments, the server path for GIFs and stickers).
#[derive(Default)]
pub struct Cache {
    items: HashMap<String, Media>,
    order: VecDeque<String>,
}

impl Cache {
    pub fn get(&self, key: &str) -> Option<&Media> {
        self.items.get(key)
    }

    pub fn frames(&self, key: &str) -> Option<&Arc<Frames>> {
        match self.items.get(key) {
            Some(Media::Ready(f)) => Some(f),
            _ => None,
        }
    }

    /// Marks a key as being fetched; false if it's already known.
    pub fn begin(&mut self, key: &str) -> bool {
        if self.items.contains_key(key) {
            return false;
        }
        self.items.insert(key.to_string(), Media::Loading);
        self.order.push_back(key.to_string());
        while self.order.len() > CACHE_SIZE {
            if let Some(old) = self.order.pop_front() {
                self.items.remove(&old);
            }
        }
        true
    }

    pub fn finish(&mut self, key: String, result: Result<Arc<Frames>, String>) {
        if !self.items.contains_key(&key) {
            return; // evicted meanwhile
        }
        self.items.insert(key, result.map_or(Media::Failed, Media::Ready));
    }

    pub fn forget(&mut self, key: &str) {
        self.items.remove(key);
        self.order.retain(|k| k != key);
    }

    /// (pictures, frames, bytes of decoded pixels) — for App settings → About.
    pub fn stats(&self) -> (usize, usize, u64) {
        self.items.values().fold((0, 0, 0), |(n, fr, b), m| match m {
            Media::Ready(f) => (n + 1, fr + f.frames.len(), b + f.frames.len() as u64 * f.width as u64 * f.height as u64 * 4),
            _ => (n, fr, b),
        })
    }

    /// Anything moving among these keys (the clock needs to tick)?
    pub fn any_animated<'a>(&self, keys: impl IntoIterator<Item = &'a str>) -> bool {
        keys.into_iter().any(|k| self.frames(k).is_some_and(|f| f.animated()))
    }
}

fn to_handle(img: DynamicImage, max: u32) -> (Handle, u32, u32) {
    let img = if img.width() > max || img.height() > max { img.thumbnail(max, max) } else { img };
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    (Handle::from_rgba(w, h, rgba.into_raw()), w, h)
}

/// Decodes a picture, every frame of an animated GIF / WebP (`still`: the first only), scaled to
/// fit `max` pixels. Run it off the UI thread.
pub fn decode(bytes: &[u8], max: u32, still: bool) -> Result<Frames, String> {
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let frames: Vec<image::Frame> = match format {
        ImageFormat::Gif if !still => {
            let d = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
            d.into_frames().take(MAX_FRAMES).collect::<Result<_, _>>().map_err(|e| e.to_string())?
        }
        ImageFormat::WebP if !still => {
            let d = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
            if d.has_animation() {
                d.into_frames().take(MAX_FRAMES).collect::<Result<_, _>>().map_err(|e| e.to_string())?
            } else {
                vec![]
            }
        }
        _ => vec![],
    };
    if frames.len() > 1 {
        let mut out = Vec::with_capacity(frames.len());
        let (mut w, mut h, mut total) = (0, 0, 0);
        for f in frames {
            let (num, den) = f.delay().numer_denom_ms();
            let ms = num.checked_div(den).map_or(100, |v| v.max(20));
            let (handle, fw, fh) = to_handle(DynamicImage::ImageRgba8(f.into_buffer()), max);
            (w, h) = (fw, fh);
            total += ms;
            out.push((handle, ms));
        }
        return Ok(Frames { frames: out, width: w, height: h, total_ms: total });
    }
    let img = image::load_from_memory_with_format(bytes, format).map_err(|e| e.to_string())?;
    let (handle, w, h) = to_handle(img, max);
    Ok(Frames { frames: vec![(handle, 0)], width: w, height: h, total_ms: 0 })
}

pub async fn decode_async(bytes: Vec<u8>, max: u32, still: bool) -> Result<Arc<Frames>, String> {
    tokio::task::spawn_blocking(move || decode(&bytes, max, still).map(Arc::new)).await.map_err(|e| e.to_string())?
}

/// `data:<mime>;base64,<data>` → (mime, bytes).
pub fn parse_data_url(url: &str) -> Option<(String, Vec<u8>)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mime = meta.split(';').next().unwrap_or("application/octet-stream").to_string();
    if !meta.ends_with(";base64") {
        return None;
    }
    Some((mime, base64::engine::general_purpose::STANDARD.decode(data.trim()).ok()?))
}

pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// A file picked to send.
#[derive(Debug, Clone)]
pub struct PendingFile {
    pub name: String,
    pub size: u64,
    pub mime: String,
    pub data_url: String,
    pub is_image: bool,
}

impl PendingFile {
    /// The message type the web uses for it.
    pub fn message_type(&self) -> &'static str {
        let lower = self.name.to_lowercase();
        let ext = |list: &[&str]| list.iter().any(|e| lower.ends_with(e));
        if self.is_image {
            "image"
        } else if self.mime.starts_with("audio/") || ext(&[".mp3", ".wav", ".ogg", ".m4a", ".aac", ".flac"]) {
            "audio"
        } else if self.mime.starts_with("video/") || ext(&[".mp4", ".mov", ".mkv", ".avi"]) {
            "video"
        } else {
            "file"
        }
    }
}

/// Reads a file to attach (off the UI thread): up to 10 MB.
pub async fn read_attachment(path: PathBuf) -> Result<PendingFile, String> {
    tokio::task::spawn_blocking(move || {
        let meta = std::fs::metadata(&path).map_err(|e| format!("Can't read {}: {e}", path.display()))?;
        if meta.is_dir() {
            return Err("That's a folder — pick a file.".to_string());
        }
        if meta.len() > MAX_FILE_BYTES {
            return Err("File size cannot exceed 10 MB".into());
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("Can't read {}: {e}", path.display()))?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        let mime = mime_guess::from_path(&path).first().map(|m| m.essence_str().to_string()).unwrap_or_else(|| "application/octet-stream".into());
        let is_image = mime.starts_with("image/") && image::guess_format(&bytes).is_ok();
        Ok(PendingFile { name, size: bytes.len() as u64, data_url: data_url(&mime, &bytes), mime, is_image })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Where downloads go (XDG on Linux, ~/Downloads elsewhere).
pub fn downloads_dir() -> PathBuf {
    crate::components::file_picker::downloads()
}

/// Saves bytes into Downloads under `name` (or "name (1).ext" if taken); the path written.
pub async fn save_to_downloads(name: String, bytes: Vec<u8>) -> Result<PathBuf, String> {
    tokio::task::spawn_blocking(move || {
        let dir = downloads_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        // Never let a name from someone else's message step out of the folder.
        let safe: String = name.chars().map(|c| if matches!(c, '/' | '\\' | '\0') { '_' } else { c }).collect();
        let safe = safe.trim_start_matches('.').to_string();
        let safe = if safe.is_empty() { "file".to_string() } else { safe };
        let (stem, ext) = match safe.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
            _ => (safe.clone(), String::new()),
        };
        let mut path = dir.join(&safe);
        let mut n = 1;
        while path.exists() {
            path = dir.join(format!("{stem} ({n}){ext}"));
            n += 1;
        }
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(path)
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1048576.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_urls_round_trip() {
        let url = data_url("image/png", b"hello");
        assert_eq!(parse_data_url(&url), Some(("image/png".to_string(), b"hello".to_vec())));
        assert!(parse_data_url("javascript:alert(1)").is_none());
    }

    #[test]
    fn decodes_a_png() {
        let mut png = Vec::new();
        DynamicImage::new_rgba8(4, 3).write_to(&mut Cursor::new(&mut png), ImageFormat::Png).unwrap();
        let f = decode(&png, 100, false).unwrap();
        assert_eq!((f.width, f.height, f.frames.len()), (4, 3, 1));
    }
}

/// A token's picture from its URL (http(s) only, at most 1 MB), decoded small. None: no picture.
pub async fn fetch_logo(url: String) -> Option<Handle> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    static HTTP: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let http = HTTP.get_or_init(|| reqwest::Client::builder().timeout(std::time::Duration::from_secs(8)).build().unwrap_or_default());
    let res = http.get(&url).send().await.ok()?;
    if !res.status().is_success() || res.content_length().is_some_and(|n| n > 1_048_576) {
        return None;
    }
    let bytes = res.bytes().await.ok()?;
    if bytes.len() > 1_048_576 {
        return None;
    }
    let frames = decode_async(bytes.to_vec(), 96, true).await.ok()?;
    frames.frames.first().map(|(h, _)| h.clone())
}
