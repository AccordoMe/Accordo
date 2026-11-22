// SPDX-License-Identifier: GPL-3.0-or-later
//! Recording voice messages and video notes (AudioRecorderBar.tsx, VideoNoteRecorderModal.tsx),
//! through the system's GStreamer (`gst-launch-1.0`, as crate::player plays them).
//!
//! What it makes is what the web's Chrome makes, so every client plays it (Safari and iPhones
//! included): MP4 — AAC sound; a video note's H.264 picture cropped square. A voice message's
//! loudness comes back as it records (the live bars, then its waveform); a video note's picture as
//! RGBA frames (the round preview, mirrored like a mirror; the first one is its thumbnail).
//!
//! Stopping sends the recorder SIGINT: with `-e` GStreamer then finishes the file properly (an MP4
//! cut off without that can't be opened). Cancelling just kills it and deletes the file.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use iced::futures::{SinkExt, Stream};
use iced::widget::image::Handle;
use tokio::io::{AsyncBufReadExt, AsyncReadExt};

use crate::player::FRAME;

/// Longest video note (the web's), and voice message.
pub const MAX_NOTE_SECS: u64 = 60;
pub const MAX_VOICE_SECS: u64 = 300;
/// Bars a voice message's waveform has (the web's).
pub const WAVEFORM_BARS: usize = 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Voice,
    Note,
}

/// What's recorded, ready to send.
#[derive(Debug, Clone)]
pub struct Recording {
    pub kind: Kind,
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub file_name: String,
    /// Seconds.
    pub duration: u64,
    /// Voice: WAVEFORM_BARS peaks, 10–95.
    pub waveform: Vec<u32>,
    /// Note: a data:image/jpeg first frame.
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// Voice: loudness now, 0–100.
    Level(f32),
    /// Note: the preview.
    Frame(Handle),
    /// Stopped: the recording — or why there's none.
    Done(Result<Box<Recording>, String>),
}

/// The recorder running now (its process id), for `stop`.
static ACTIVE: Mutex<Option<u32>> = Mutex::new(None);

/// Ends the recording and keeps it (SIGINT: GStreamer finishes the file).
pub fn stop() {
    let pid = ACTIVE.lock().ok().and_then(|a| *a);
    if let Some(pid) = pid {
        let _ = std::process::Command::new("kill").arg("-INT").arg(pid.to_string()).status();
    }
}

/// Ends the recording and throws it away (no `-e`: the half-written file is no use).
pub fn cancel() {
    if let Some(pid) = ACTIVE.lock().ok().and_then(|mut a| a.take()) {
        let _ = std::process::Command::new("kill").arg("-KILL").arg(pid.to_string()).status();
    }
}

/// Where a recording is written while it's made (the user's runtime directory, as the player's:
/// a private tmpfs on Linux).
pub fn temp_file() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(std::env::temp_dir);
    let dir = base.join("accordo-media");
    let _ = std::fs::create_dir_all(&dir);
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    dir.join(format!("recording-{n}.mp4"))
}

fn has(gst: &Path, element: &str) -> bool {
    let inspect = gst.with_file_name(if cfg!(windows) { "gst-inspect-1.0.exe" } else { "gst-inspect-1.0" });
    std::process::Command::new(inspect).arg("--exists").arg(element).status().is_ok_and(|s| s.success())
}

/// The AAC encoder this GStreamer has.
fn aac(gst: &Path) -> Option<&'static str> {
    ["avenc_aac", "voaacenc", "fdkaacenc"].into_iter().find(|e| has(gst, e))
}

fn h264(gst: &Path) -> Option<&'static str> {
    if has(gst, "x264enc") {
        Some("x264enc tune=zerolatency speed-preset=veryfast bitrate=800 key-int-max=60 ! video/x-h264,profile=main")
    } else if has(gst, "openh264enc") {
        Some("openh264enc bitrate=800000 ! video/x-h264,profile=baseline")
    } else {
        None
    }
}

fn camera() -> &'static str {
    if Path::new("/dev/video0").exists() { "v4l2src" } else { "autovideosrc" }
}

/// The pipeline (gst-launch words) for `kind`, writing `file`; None: an encoder is missing.
pub fn pipeline(gst: &Path, kind: Kind, file: &Path) -> Result<Vec<String>, String> {
    let aac = aac(gst).ok_or("GStreamer here has no AAC encoder (install gstreamer1.0-libav)")?;
    let loc = format!("location={}", file.display());
    let audio = format!("autoaudiosrc ! audioconvert ! audioresample ! audio/x-raw,rate=48000,channels=1 ! {aac} bitrate=64000");
    let text = match kind {
        Kind::Voice => format!(
            "{audio} ! mp4mux faststart=true ! filesink {loc} \
             autoaudiosrc ! audioconvert ! level interval=100000000 post-messages=true ! fakesink"
        ),
        Kind::Note => {
            let h264 = h264(gst).ok_or("GStreamer here has no H.264 encoder (install gstreamer1.0-plugins-ugly)")?;
            format!(
                "{cam} ! decodebin ! videoconvert ! videorate ! video/x-raw,framerate=30/1 ! aspectratiocrop aspect-ratio=1/1 ! videoscale ! video/x-raw,width=480,height=480 ! tee name=v \
                 v. ! queue ! videoconvert ! {h264} ! h264parse ! mux. \
                 v. ! queue leaky=downstream max-size-buffers=2 ! videoflip method=horizontal-flip ! videoscale ! videoconvert ! video/x-raw,format=RGBA,width={FRAME},height={FRAME} ! fdsink fd=1 sync=false \
                 {audio} ! mux. \
                 mp4mux name=mux faststart=true ! filesink {loc}",
                cam = camera(),
            )
        }
    };
    // The tokens gst-launch takes (its own syntax; nothing here needs quoting).
    Ok(text.split_whitespace().map(str::to_string).collect())
}

/// "peak=(GValueArray)< -15.6 >" in a level message → 10–95.
pub fn level_of(line: &str) -> Option<f32> {
    let i = line.find("peak=")?;
    let rest = &line[i..];
    let open = rest.find('<')?;
    let close = rest.find('>')?;
    let db: f32 = rest[open + 1..close].split(',').next()?.trim().parse().ok()?;
    // -50 dB (quiet room) … 0 dB (loud): the web's 10–95.
    Some((10.0 + (db + 50.0) / 50.0 * 85.0).clamp(10.0, 95.0))
}

/// The recorded peaks squeezed (or stretched) to WAVEFORM_BARS (the web's averaging).
pub fn waveform(peaks: &[f32]) -> Vec<u32> {
    if peaks.is_empty() {
        return vec![30; WAVEFORM_BARS];
    }
    if peaks.len() <= WAVEFORM_BARS {
        let mut out: Vec<u32> = peaks.iter().map(|p| p.round() as u32).collect();
        while out.len() < WAVEFORM_BARS {
            out.push(*out.last().unwrap_or(&30));
        }
        return out;
    }
    let block = peaks.len() as f32 / WAVEFORM_BARS as f32;
    (0..WAVEFORM_BARS)
        .map(|i| {
            let slice = &peaks[(i as f32 * block) as usize..(((i + 1) as f32 * block) as usize).min(peaks.len())];
            let avg = if slice.is_empty() { 30.0 } else { slice.iter().sum::<f32>() / slice.len() as f32 };
            avg.round().clamp(10.0, 95.0) as u32
        })
        .collect()
}

fn thumbnail(rgba: &[u8]) -> Option<String> {
    let img = image::RgbaImage::from_raw(FRAME, FRAME, rgba.to_vec())?;
    let small = image::imageops::resize(&image::DynamicImage::ImageRgba8(img).to_rgb8(), 240, 240, image::imageops::FilterType::Triangle);
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80).encode_image(&small).ok()?;
    Some(crate::media::data_url("image/jpeg", &out))
}

/// Records until `stop()` (or the stream is dropped: cancelled — the process dies with it).
pub fn record(gst: &Path, kind: Kind, file: PathBuf) -> impl Stream<Item = Event> + use<> {
    let gst = gst.to_path_buf();
    iced::stream::channel(8, async move |mut out: iced::futures::channel::mpsc::Sender<Event>| {
        let args = match pipeline(&gst, kind, &file) {
            Ok(a) => a,
            Err(e) => {
                let _ = out.send(Event::Done(Err(e))).await;
                return;
            }
        };
        let mut cmd = tokio::process::Command::new(&gst);
        cmd.arg("-e").arg(if kind == Kind::Voice { "-m" } else { "-q" }).args(&args);
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).kill_on_drop(true);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = out.send(Event::Done(Err(format!("Couldn't start recording: {e}")))).await;
                return;
            }
        };
        if let Ok(mut a) = ACTIVE.lock() {
            *a = child.id();
        }
        let started = Instant::now();
        let mut peaks: Vec<f32> = Vec::new();
        let mut thumb: Option<String> = None;
        if let Some(stdout) = child.stdout.take() {
            match kind {
                Kind::Voice => {
                    let mut lines = tokio::io::BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if let Some(l) = line.contains("level, endtime").then(|| level_of(&line)).flatten() {
                            peaks.push(l);
                            let _ = out.send(Event::Level(l)).await;
                        }
                    }
                }
                Kind::Note => {
                    let mut stdout = stdout;
                    let mut buf = vec![0u8; (FRAME * FRAME * 4) as usize];
                    while stdout.read_exact(&mut buf).await.is_ok() {
                        if thumb.is_none() {
                            thumb = thumbnail(&buf);
                        }
                        let _ = out.send(Event::Frame(Handle::from_rgba(FRAME, FRAME, buf.clone()))).await;
                    }
                }
            }
        }
        let _ = child.wait().await;
        if let Ok(mut a) = ACTIVE.lock() {
            *a = None;
        }
        let duration = started.elapsed().as_secs().max(1);
        let bytes = std::fs::read(&file).unwrap_or_default();
        let _ = std::fs::remove_file(&file);
        let result = if bytes.len() < 1000 {
            Err(match kind {
                Kind::Voice => "Nothing was recorded — is a microphone connected and allowed?".to_string(),
                Kind::Note => "Nothing was recorded — is the camera free (not used by another app)?".to_string(),
            })
        } else if bytes.len() as u64 > crate::media::MAX_FILE_BYTES {
            Err("It's too long to send (over 10 MB)".to_string())
        } else {
            let stamp = chrono::Local::now().format("%H-%M-%S");
            Ok(Box::new(match kind {
                Kind::Voice => Recording { kind, bytes, mime: "audio/mp4", file_name: format!("VoiceMessage_{stamp}.m4a"), duration, waveform: waveform(&peaks), thumbnail: None },
                Kind::Note => Recording { kind, bytes, mime: "video/mp4", file_name: format!("VideoNote_{stamp}.mp4"), duration, waveform: Vec::new(), thumbnail: thumb },
            }))
        };
        let _ = out.send(Event::Done(result)).await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_waveform() {
        let line = r#"Got message #115 from element "level0" (element): level, endtime=(guint64)102858895, rms=(GValueArray)< -26.78 >, peak=(GValueArray)< -15.6 >, decay=(GValueArray)< -15.6 >;"#;
        let l = level_of(line).unwrap();
        assert!((68.0..=69.0).contains(&l), "{l}");
        assert_eq!(level_of("no level here"), None);
        assert_eq!(level_of("peak=(GValueArray)< -90 >"), Some(10.0)); // silence: the lowest bar
        // A live message from a stereo source: two values, the first one is the loudness.
        let stereo = r#"Got message #144 from element "level0" (element): level, endtime=(guint64)120000000, rms=(GValueArray)< -18.63, -18.47 >, peak=(GValueArray)< -7.581, -7.467 >, decay=(GValueArray)< -7.581, -7.467 >;"#;
        assert!(matches!(level_of(stereo), Some(v) if (82.0..=82.5).contains(&v)), "{:?}", level_of(stereo));
        assert_eq!(waveform(&[]).len(), WAVEFORM_BARS);
        assert_eq!(waveform(&[50.0; 10]), vec![50; WAVEFORM_BARS]);
        let long: Vec<f32> = (0..280).map(|i| if i < 140 { 20.0 } else { 80.0 }).collect();
        let w = waveform(&long);
        assert_eq!((w.len(), w[0], w[27]), (WAVEFORM_BARS, 20, 80));
    }
}
