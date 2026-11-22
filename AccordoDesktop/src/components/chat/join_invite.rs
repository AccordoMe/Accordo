// SPDX-License-Identifier: GPL-3.0-or-later
//! Joining a group with its invite link (the web: ChatPage's `?invite=` flow).
//!
//! Paste the link → join. The link's `#k=` secret never goes to the server: it only MACs who's
//! joining (`invite_tag`), so the members' apps can tell the join came from someone with the link.
//! An approval-only link doesn't let you in: the server files a request for the group's admins —
//! then the window turns into "Request to join", where a note for them can go (once; the web's
//! "Add a note for the admins?"), or the request can be taken back.

use iced::widget::{button, column, container, row, space, text, text_editor, text_input};
use iced::{Alignment, Color, Fill};

use crate::app::Message;
use crate::components::overlay;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::chat_page::ChatMsg;
use crate::theme::*;

/// Same limit as the server (app/join_requests.MESSAGE_MAX) and the web (NOTE_MAX).
pub const NOTE_MAX: usize = 500;

#[derive(Debug, Clone)]
pub enum JoinMsg {
    Open,
    Close,
    Link(String),
    Join,
    Note(text_editor::Action),
    SendNote,
    Withdraw,
    Joined(Result<Box<crate::api::chat::ChatRoomItem>, crate::api::client::ApiError>),
    Noted(Result<(), String>),
    Withdrawn(Result<(), String>),
}

/// What a pasted invite link holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub code: String,
    /// The `#k=` secret, when the link has one.
    pub secret: Option<String>,
    /// Where the link points ("https://accordo.me"); None for a bare code.
    pub origin: Option<String>,
}

fn is_code(s: &str) -> bool {
    (4..=64).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A full link (`…/chat?invite=CODE#k=SECRET`, with or without the scheme) or just the code.
pub fn parse(input: &str) -> Option<Invite> {
    let s = input.trim();
    if is_code(s) {
        return Some(Invite { code: s.to_string(), secret: None, origin: None });
    }
    let with_scheme = if s.contains("://") { s.to_string() } else { format!("https://{s}") };
    let url = reqwest::Url::parse(&with_scheme).ok()?;
    let code = url.query_pairs().find(|(k, _)| k == "invite").map(|(_, v)| v.into_owned()).filter(|c| is_code(c))?;
    let secret = url.fragment().and_then(|f| f.split('&').find_map(|p| p.strip_prefix("k="))).filter(|v| !v.is_empty()).map(str::to_string);
    let origin = url.host_str().map(|h| match url.port() {
        Some(p) => format!("{}://{h}:{p}", url.scheme()),
        None => format!("{}://{h}", url.scheme()),
    });
    Some(Invite { code, secret, origin })
}

/// The request filed through an approval-only link.
pub struct Requested {
    pub room_key: String,
    pub title: Option<String>,
    pub note: text_editor::Content,
    /// The note went with it: it can't be changed (withdraw and ask again).
    pub note_sent: bool,
}

#[derive(Default)]
pub struct State {
    pub link: String,
    pub busy: bool,
    pub error: Option<String>,
    pub requested: Option<Requested>,
}

impl State {
    pub fn note_len(&self) -> usize {
        self.requested.as_ref().map(|r| r.note.text().trim_end().chars().count()).unwrap_or(0)
    }
}

fn m(j: JoinMsg) -> Message {
    Message::Chat(ChatMsg::JoinInvite(j))
}

fn header<'a>(handle: iced::widget::svg::Handle, title: String, sub: String) -> El<'a> {
    row![
        container(icon(handle, 17, INDIGO_SOFT)).width(36).height(36).center(36).style(boxed(rgba(0x6366f1, 0.15), rgba(0x6366f1, 0.35), 10.0)),
        column![text(title).size(16).font(BOLD).color(Color::WHITE), text(sub).size(12).color(TEXT_SECONDARY)].spacing(2).width(Fill),
        button(icon(icons::x(), 16, MUTED)).padding(2).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(m(JoinMsg::Close)),
    ]
    
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

fn error_line<'a>(e: &str) -> El<'a> {
    container(text(e.to_string()).size(12).color(DANGER_TEXT)).width(Fill).padding([8, 10]).style(boxed(rgba(0xef4444, 0.08), rgba(0xef4444, 0.3), 8.0)).into()
}

pub fn view(st: &State) -> El<'_> {
    let body = match &st.requested {
        None => paste_view(st),
        Some(r) => request_view(st, r),
    };
    overlay::modal(container(body).width(480).padding(20).style(modal), m(JoinMsg::Close))
}

fn paste_view(st: &State) -> El<'_> {
    let ready = !st.busy && parse(&st.link).is_some();
    let mut col = column![
        header(icons::link_2(), "Join with a link".into(), "A group's invite link — or just its code".into()),
        text_input("https://accordo.me/chat?invite=…", &st.link)
            .size(13)
            .padding([10, 12])
            .style(field)
            .on_input(|v| m(JoinMsg::Link(v)))
            .on_submit_maybe(ready.then(|| m(JoinMsg::Join))),
    ]
    .spacing(14);
    if !st.link.trim().is_empty() && parse(&st.link).is_none() {
        col = col.push(text("That doesn't look like an invite link").size(11.5).color(AMBER_TEXT));
    }
    if let Some(e) = &st.error {
        col = col.push(error_line(e));
    }
    col = col.push(text("Some groups let you in right away; others ask their admins first — then you can add a note for them.").size(11.5).color(FAINT).line_height(1.45));
    col.push(row![
        space().width(Fill),
        button(text("Cancel").size(13)).padding([8, 16]).style(tone_button(Tone::Dark)).on_press(m(JoinMsg::Close)),
        button(text(if st.busy { "Joining…" } else { "Join" }).size(13).font(SEMIBOLD)).padding([8, 20]).style(primary).on_press_maybe(ready.then(|| m(JoinMsg::Join))),
    ]
    .spacing(8))
    .into()
}

fn request_view<'a>(st: &'a State, r: &'a Requested) -> El<'a> {
    let name = r.title.as_deref().filter(|t| !t.is_empty()).unwrap_or("the group");
    let sent = container(
        row![icon(icons::check(), 14, GREEN), text(format!("Request sent — the admins of {name} will let you in")).size(12.5).font(SEMIBOLD).color(hex(0x34d399))].spacing(8).align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([10, 12])
    .style(boxed(rgba(0x10b981, 0.1), rgba(0x10b981, 0.35), 10.0));
    let mut col = column![header(icons::user_plus(), "Request to join".into(), r.title.clone().unwrap_or_else(|| "Group".into())), sent].spacing(14);

    if r.note_sent {
        col = col.push(text("Your note is with the admins. The group opens here once they let you in.").size(12).color(TEXT_SECONDARY).line_height(1.45));
    } else {
        let len = st.note_len();
        col = col.push(
            column![
                text("Add a note for the admins?").size(13).font(SEMIBOLD).color(Color::WHITE),
                text_editor(&r.note)
                    .placeholder("Why should they let you in? (optional)")
                    .height(96)
                    .size(13)
                    .padding(10)
                    .style(crate::components::chain_settings::editor_style)
                    .on_action(|a| m(JoinMsg::Note(a))),
                row![
                    text("The group's admins read this. It isn't encrypted — don't put anything secret here.").size(11).color(FAINT).width(Fill),
                    text(format!("{len}/{NOTE_MAX}")).size(11).color(if len > NOTE_MAX { DANGER_TEXT } else { FAINT }),
                ]
                .spacing(8),
            ]
            .spacing(8),
        );
    }
    if let Some(e) = &st.error {
        col = col.push(error_line(e));
    }
    let len = st.note_len();
    let can_send = !st.busy && !r.note_sent && len > 0 && len <= NOTE_MAX;
    let mut buttons = row![
        button(text("Withdraw request").size(12.5).color(DANGER_TEXT)).padding([8, 12]).style(ghost(8.0, rgba(0xef4444, 0.1))).on_press_maybe((!st.busy).then(|| m(JoinMsg::Withdraw))),
        space().width(Fill),
        button(text(if r.note_sent { "Done" } else { "Skip" }).size(13)).padding([8, 16]).style(tone_button(Tone::Dark)).on_press(m(JoinMsg::Close)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if !r.note_sent {
        buttons = buttons.push(button(text(if st.busy { "Sending…" } else { "Send note" }).size(13).font(SEMIBOLD)).padding([8, 18]).style(primary).on_press_maybe(can_send.then(|| m(JoinMsg::SendNote))));
    }
    col.push(buttons).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_and_codes() {
        let i = parse("https://accordo.me/chat?invite=AbC-12_x#k=s3cr_et").unwrap();
        assert_eq!(i, Invite { code: "AbC-12_x".into(), secret: Some("s3cr_et".into()), origin: Some("https://accordo.me".into()) });
        let i = parse("  http://127.0.0.1:5173/chat?invite=AbC-12_x ").unwrap();
        assert_eq!((i.secret, i.origin.as_deref()), (None, Some("http://127.0.0.1:5173")));
        assert_eq!(parse("accordo.me/chat?invite=Zz9_Zz9").unwrap().code, "Zz9_Zz9");
        assert_eq!(parse("Zz9_Zz9").unwrap().origin, None);
        assert!(parse("https://accordo.me/chat").is_none());
        assert!(parse("hello there").is_none());
        assert!(parse("https://x.y/chat?invite=bad%20code").is_none());
    }
}
