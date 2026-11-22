// SPDX-License-Identifier: GPL-3.0-or-later
//! Voice and video messages played in the chat (AudioMessagePlayer.tsx, VideoNotePlayer.tsx).
//!
//! The web records whatever its browser makes — MP4 (H.264 / AAC) in Chrome, WebM (VP8/VP9 /
//! Opus) in Firefox — so playing them takes a real media framework. This uses the system's
//! GStreamer through its `gst-launch-1.0` (no link to it: the app runs without it, and on a computer
//! without it ▶ opens the file in the system's player instead). GStreamer plays the sound itself;
//! a video's frames come back on its stdout as RGBA squares, in real time (`sync=true`).
//!
//! The file is written for the player to read to the user's runtime directory (a private tmpfs on
//! Linux) and deleted when playback ends.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use iced::futures::{SinkExt, Stream};
use iced::widget::image::Handle;
use tokio::io::AsyncReadExt;

/// A video frame's side, in pixels (shown at 240 logical px: sharp on a 1.3× screen).
pub const FRAME: u32 = 320;

#[derive(Debug, Clone)]
pub enum Event {
    Frame(Handle),
    /// Finished (or failed: why).
    Done(Result<(), String>),
}

/// The system's `gst-launch-1.0`, if there is one.
pub fn gst() -> Option<&'static Path> {
    static GST: OnceLock<Option<PathBuf>> = OnceLock::new();
    GST.get_or_init(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path).map(|d| d.join(if cfg!(windows) { "gst-launch-1.0.exe" } else { "gst-launch-1.0" })).find(|p| p.is_file())
    })
    .as_deref()
}

fn media_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(std::env::temp_dir);
    base.join("accordo-media")
}

/// The extension for a MIME type ("audio/webm;codecs=opus" → "webm").
pub fn extension(mime: &str) -> &'static str {
    let m = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    match m.as_str() {
        "audio/webm" | "video/webm" => "webm",
        "audio/ogg" | "video/ogg" => "ogg",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/mpeg" => "mp3",
        "audio/aac" => "aac",
        "video/quicktime" => "mov",
        _ => "mp4",
    }
}

/// Writes a message's media where the player can read it (private to the user).
pub fn write_temp(message_id: i64, bytes: &[u8], ext: &str) -> std::io::Result<PathBuf> {
    let dir = media_dir();
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let path = dir.join(format!("message-{message_id}.{ext}"));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    opts.open(&path)?.write_all(bytes)?;
    Ok(path)
}

/// `file:///…` for GStreamer (spaces and the like escaped).
fn file_uri(path: &Path) -> String {
    let p = path.to_string_lossy();
    let mut out = String::from("file://");
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Plays a file; with `video`, its frames come back as events. Dropping the stream stops it (the
/// process is killed with it).
pub fn play(gst: &Path, file: PathBuf, video: bool) -> impl Stream<Item = Event> + use<> {
    let gst = gst.to_path_buf();
    iced::stream::channel(4, async move |mut out: iced::futures::channel::mpsc::Sender<Event>| {
        let mut cmd = tokio::process::Command::new(&gst);
        cmd.arg("-q").arg("playbin").arg(format!("uri={}", file_uri(&file)));
        if video {
            cmd.arg(format!("video-sink=videoconvert ! aspectratiocrop aspect-ratio=1/1 ! videoscale ! video/x-raw,format=RGBA,width={FRAME},height={FRAME} ! fdsink fd=1 sync=true"));
            cmd.stdout(std::process::Stdio::piped());
        } else {
            cmd.arg("video-sink=fakesink").stdout(std::process::Stdio::null());
        }
        // stderr isn't read: a chatty GStreamer would fill the pipe and stall.
        cmd.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).kill_on_drop(true);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = out.send(Event::Done(Err(format!("Couldn't start the player: {e}")))).await;
                return;
            }
        };
        if let Some(mut stdout) = child.stdout.take() {
            let mut buf = vec![0u8; (FRAME * FRAME * 4) as usize];
            while stdout.read_exact(&mut buf).await.is_ok() {
                if out.send(Event::Frame(Handle::from_rgba(FRAME, FRAME, buf.clone()))).await.is_err() {
                    return; // nobody's watching: stop (kill_on_drop)
                }
            }
        }
        let status = child.wait().await;
        let _ = std::fs::remove_file(&file);
        let result = match status {
            Ok(s) if s.success() => Ok(()),
            _ => Err("This file couldn't be played here — try Save".to_string()),
        };
        let _ = out.send(Event::Done(result)).await;
    })
}

/// "0:05", "1:23".
pub fn clock(seconds: f32) -> String {
    let s = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_and_times() {
        assert_eq!(super::extension("audio/webm;codecs=opus"), "webm");
        assert_eq!(super::extension("video/mp4"), "mp4");
        assert_eq!(super::extension("audio/ogg"), "ogg");
        assert_eq!(super::clock(5.4), "0:05");
        assert_eq!(super::clock(83.0), "1:23");
        assert_eq!(super::file_uri(std::path::Path::new("/run/a b/x.webm")), "file:///run/a%20b/x.webm");
    }
}
