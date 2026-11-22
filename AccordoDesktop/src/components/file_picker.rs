// SPDX-License-Identifier: GPL-3.0-or-later
//! Picking a file, drawn by the app in its own style (instead of the system dialog): places on the
//! left, the folder's contents on the right, folders first; only the kinds of file asked for.

use std::path::{Path, PathBuf};

use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, space, svg, text, Column};
use iced::{Alignment, Color, Fill, Font};

use super::overlay;
use super::ui::{icon, truncate, El};
use crate::app::Message;
use crate::icons;
use crate::theme::{self, *};

/// What the picked file is for (the app acts on it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    ChatKeyFile,
    /// A file to send in the open chat (any kind, up to 10 MB).
    Attachment,
    /// A drop's wallet list (CSV / JSON / text).
    #[cfg(feature = "drops")]
    DropList,
    /// A picture for the open Manage Room's group.
    RoomAvatar,
    /// A picture for the account.
    ProfileAvatar,
    /// An organization's logo (the new one's, or one I own).
    OrgLogo,
}

const PICTURES: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];

#[derive(Debug, Clone)]
pub enum PickerMsg {
    Open(Purpose),
    Go(PathBuf),
    Select(PathBuf),
    Confirm,
    Close,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<chrono::DateTime<chrono::Local>>,
}

#[derive(Debug, Clone)]
pub struct State {
    pub purpose: Purpose,
    pub dir: PathBuf,
    pub items: Vec<Item>,
    pub selected: Option<PathBuf>,
    pub error: Option<String>,
}

impl Purpose {
    fn title(self) -> &'static str {
        match self {
            Purpose::ChatKeyFile => "Load your chat key file",
            Purpose::Attachment => "Attach a file",
            Purpose::RoomAvatar => "The room's picture",
            Purpose::ProfileAvatar => "Your picture",
            Purpose::OrgLogo => "The organization's logo",
            #[cfg(feature = "drops")]
            Purpose::DropList => "Load a wallet list",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Purpose::ChatKeyFile => "The .json file downloaded with \"Download key file\" (accordo_e2ee_keys_…).",
            Purpose::Attachment => "Up to 10 MB. In private chats it's encrypted before it leaves this computer.",
            Purpose::RoomAvatar | Purpose::ProfileAvatar | Purpose::OrgLogo => "PNG, JPEG, WebP, GIF or BMP — made 160 px before it's sent. Everyone who sees it can see it: it isn't encrypted.",
            #[cfg(feature = "drops")]
            Purpose::DropList => "CSV lines `address, amount, name, plan` or JSON [{address, amount, name, plan}], up to 20 MB.",
        }
    }

    fn extensions(self) -> &'static [&'static str] {
        match self {
            Purpose::ChatKeyFile => &["json"],
            Purpose::Attachment => &[],
            Purpose::RoomAvatar | Purpose::ProfileAvatar | Purpose::OrgLogo => PICTURES,
            #[cfg(feature = "drops")]
            Purpose::DropList => &["csv", "json", "txt", "tsv"],
        }
    }
}

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// Where downloads usually are: XDG's setting on Linux, ~/Downloads elsewhere.
pub fn downloads() -> PathBuf {
    let home = home();
    if let Ok(dirs) = std::fs::read_to_string(home.join(".config/user-dirs.dirs")) {
        for line in dirs.lines() {
            if let Some(v) = line.strip_prefix("XDG_DOWNLOAD_DIR=") {
                let p = v.trim_matches('"').replace("$HOME", &home.to_string_lossy());
                if Path::new(&p).is_dir() {
                    return PathBuf::from(p);
                }
            }
        }
    }
    home.join("Downloads")
}

impl State {
    /// Opens in Downloads (where a key file usually is), else the home folder.
    pub fn new(purpose: Purpose) -> Self {
        let start = Some(downloads()).filter(|d| d.is_dir()).unwrap_or_else(home);
        let mut s = Self { purpose, dir: start.clone(), items: Vec::new(), selected: None, error: None };
        s.go(start);
        s
    }

    pub fn go(&mut self, dir: PathBuf) {
        match std::fs::read_dir(&dir) {
            Ok(read) => {
                let exts = self.purpose.extensions();
                let mut items: Vec<Item> = read
                    .flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().into_owned();
                        if name.starts_with('.') {
                            return None;
                        }
                        let meta = e.metadata().ok()?;
                        let is_dir = meta.is_dir();
                        let ext_ok = exts.is_empty() || e.path().extension().and_then(|x| x.to_str()).is_some_and(|x| exts.iter().any(|w| w.eq_ignore_ascii_case(x)));
                        (is_dir || ext_ok).then(|| Item {
                            path: e.path(),
                            name,
                            is_dir,
                            size: meta.len(),
                            modified: meta.modified().ok().map(chrono::DateTime::<chrono::Local>::from),
                        })
                    })
                    .collect();
                items.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
                self.items = items;
                self.dir = dir;
                self.selected = None;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Can't open {}: {e}", dir.display())),
        }
    }

    pub fn update(&mut self, msg: PickerMsg) {
        match msg {
            PickerMsg::Go(dir) => self.go(dir),
            PickerMsg::Select(path) => self.selected = Some(path),
            PickerMsg::Open(_) | PickerMsg::Confirm | PickerMsg::Close => {}
        }
    }
}

fn file_icon(purpose: Purpose, item: &Item) -> svg::Handle {
    if item.is_dir {
        return icons::briefcase();
    }
    if purpose == Purpose::ChatKeyFile {
        return icons::key_round();
    }
    let lower = item.name.to_lowercase();
    if [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"].iter().any(|e| lower.ends_with(e)) { icons::image() } else { icons::paperclip() }
}

fn size(bytes: u64) -> String {
    if bytes < 1024 { format!("{bytes} B") } else if bytes < 1024 * 1024 { format!("{:.1} kB", bytes as f64 / 1024.0) } else { format!("{:.1} MB", bytes as f64 / 1048576.0) }
}

pub fn view(state: &State) -> El<'_> {
    let msg = Message::Picker;
    let header = row![
        icon(icons::upload(), 17, INDIGO_SOFT),
        text(state.purpose.title()).size(16).font(BOLD).color(Color::WHITE),
        space().width(Fill),
        button(icon(icons::x(), 16, MUTED)).padding(2).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(msg(PickerMsg::Close)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let place = |label: &'static str, handle: svg::Handle, path: PathBuf| -> El<'_> {
        let here = state.dir == path;
        button(row![icon(handle, 14, if here { Color::WHITE } else { MUTED }), text(label).size(12.5)].spacing(8).align_y(Alignment::Center))
            .width(Fill)
            .padding([7, 10])
            .style(selectable(here, rgba(0x6366f1, 0.2), 6.0))
            .on_press_maybe((path.is_dir()).then(|| msg(PickerMsg::Go(path))))
            .into()
    };
    let h = home();
    let places = column![
        place("Home", icons::user(), h.clone()),
        place("Downloads", icons::download(), downloads()),
        place("Desktop", icons::app_window(), h.join("Desktop")),
        place("Documents", icons::receipt(), h.join("Documents")),
    ]
    .spacing(2)
    .width(170);

    let up = state.dir.parent().map(Path::to_path_buf);
    let path_bar = row![
        button(icon(icons::arrow_up_right(), 14, MUTED)).padding([6, 8]).style(tone_button(Tone::Dark)).on_press_maybe(up.map(|p| msg(PickerMsg::Go(p)))),
        container(text(truncate(&state.dir.display().to_string(), 70)).size(12).font(Font::MONOSPACE).color(TEXT_BODY).wrapping(Wrapping::None))
            .width(Fill)
            .padding([7, 10])
            .clip(true)
            .style(boxed(hex(0x0e0f17), BORDER, 6.0)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let mut list = Column::new().spacing(1);
    if state.items.is_empty() {
        list = list.push(container(text(if state.purpose.extensions().is_empty() { "Nothing here".to_string() } else { format!("No folders or .{} files here", state.purpose.extensions().join(", .")) }).size(12).color(FAINT)).padding(14));
    }
    for item in &state.items {
        let selected = state.selected.as_ref() == Some(&item.path);
        let line = row![
            icon(file_icon(state.purpose, item), 14, if item.is_dir { INDIGO_ICON } else { AMBER_TEXT }),
            container(text(item.name.clone()).size(12.5).color(if selected { Color::WHITE } else { TEXT_BODY }).wrapping(Wrapping::None)).width(Fill).clip(true),
            text(if item.is_dir { String::new() } else { size(item.size) }).size(11).font(Font::MONOSPACE).color(FAINT).width(70),
            text(item.modified.map(|m| m.format("%b %-d, %H:%M").to_string()).unwrap_or_default()).size(11).color(FAINT).width(96),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let action = if item.is_dir { PickerMsg::Go(item.path.clone()) } else { PickerMsg::Select(item.path.clone()) };
        list = list.push(button(line).width(Fill).padding([6, 10]).style(selectable(selected, rgba(0x6366f1, 0.22), 6.0)).on_press(msg(action)));
    }

    let mut right = column![path_bar, container(scrollable(list).height(Fill).style(thin_scroll)).height(320).style(boxed(rgba(0xffffff, 0.015), BORDER, 8.0))].spacing(8).width(Fill);
    if let Some(e) = &state.error {
        right = right.push(text(e.clone()).size(12).color(DANGER_TEXT));
    }

    let chosen = state.selected.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
    let footer = row![
        text(chosen.clone().map_or(state.purpose.hint().to_string(), |n| format!("Selected: {n}"))).size(12).color(if chosen.is_some() { TEXT_BODY } else { FAINT }).width(Fill),
        button(text("Cancel").size(13)).padding([8, 16]).style(secondary).on_press(msg(PickerMsg::Close)),
        button(text("Open").size(13).font(SEMIBOLD)).padding([8, 18]).style(tone_button(Tone::Accent)).on_press_maybe(chosen.is_some().then(|| msg(PickerMsg::Confirm))),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let body = column![header, row![places, container(space().width(1).height(Fill)).height(360).style(theme::rule), right].spacing(12), footer].spacing(14);
    overlay::modal(container(body).width(820).padding(20).style(modal), msg(PickerMsg::Close))
}
