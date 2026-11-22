// SPDX-License-Identifier: GPL-3.0-or-later
//! The message box at the bottom of a conversation. In an encrypted chat whose key isn't on this
//! computer it's a note saying why instead: plaintext must never go into an encrypted chat.

use iced::widget::{button, center, container, row, svg, text, text_input};
use iced::{Alignment, Color, Fill, Padding};

use super::display;
use crate::api::chat::{ChatMessageItem, ChatRoomItem};
use crate::e2ee::rooms::KeyProblem;
use crate::app::Message;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::chat_page::{ChatMsg, Recording, COMPOSER};
use super::attach_picker::FinanceKind;
use crate::theme::*;

/// What the box is doing besides writing: answering a message, or changing one of yours.
pub enum Mode<'a> {
    Write,
    Reply(&'a ChatMessageItem),
    Edit,
}

/// The strip above the box: "Replying to @alina" / "Editing message", with a ✕.
fn strip<'a>(mode: &Mode<'a>) -> Option<El<'a>> {
    let (handle, label, detail, cancel) = match mode {
        Mode::Write => return None,
        Mode::Reply(m) => {
            let who = m.sender_display_name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| m.sender_username.clone());
            let (said, _) = display::message_text(m);
            (icons::reply(), format!("Replying to @{who}"), said.replace('\n', " "), ChatMsg::CancelReply)
        }
        Mode::Edit => (icons::pencil(), "Editing message".to_string(), "Enter to save · Esc to cancel".to_string(), ChatMsg::CancelEdit),
    };
    Some(
        container(
            row![
                icon(handle, 13, INDIGO_SOFT),
                text(label).size(12.5).font(SEMIBOLD).color(INDIGO_SOFT),
                text(crate::components::ui::truncate(&detail, 80)).size(12).color(MUTED).wrapping(iced::widget::text::Wrapping::None).width(Fill),
                button(icon(icons::x(), 14, MUTED)).padding(3).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(Message::Chat(cancel)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([6, 12])
        .style(|_| container::Style {
            background: Some(rgba(0x6366f1, 0.08).into()),
            border: iced::Border { color: rgba(0x6366f1, 0.25), width: 1.0, radius: iced::border::Radius { top_left: 10.0, top_right: 10.0, bottom_right: 0.0, bottom_left: 0.0 } },
            ..Default::default()
        })
        .into(),
    )
}

/// The file waiting to be sent: a thumbnail (pictures) or an icon, its name and size, a ✕.
fn pending_strip<'a>(file: &'a crate::media::PendingFile, thumb: Option<&'a iced::widget::image::Handle>) -> El<'a> {
    let face: El = match thumb {
        Some(h) => container(iced::widget::image(h.clone()).width(40).height(40).content_fit(iced::ContentFit::Cover)).width(40).height(40).clip(true).style(boxed(rgba(0xffffff, 0.05), Color::TRANSPARENT, 6.0)).into(),
        None => container(center(icon(if file.is_image { icons::image() } else { icons::file_text() }, 18, INDIGO_SOFT))).width(40).height(40).style(boxed(rgba(0x6366f1, 0.15), Color::TRANSPARENT, 6.0)).into(),
    };
    container(
        row![
            face,
            iced::widget::column![
                text(crate::components::ui::truncate(&file.name, 60)).size(13).font(SEMIBOLD).color(Color::WHITE).wrapping(iced::widget::text::Wrapping::None),
                text(format!("{} · Ready to send", crate::media::human_size(file.size))).size(11.5).color(MUTED),
            ]
            .spacing(2)
            .width(Fill),
            button(icon(icons::x(), 14, MUTED)).padding(3).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(Message::Chat(ChatMsg::ClearAttachment)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([8, 12])
    .style(|_| container::Style {
        background: Some(rgba(0xffffff, 0.03).into()),
        border: iced::Border { color: BORDER, width: 1.0, radius: iced::border::Radius { top_left: 10.0, top_right: 10.0, bottom_right: 0.0, bottom_left: 0.0 } },
        ..Default::default()
    })
    .into()
}

/// The cheque / invoice waiting to be sent: "Cheque: 5 USDC", who it's for, a ✕.
fn finance_strip<'a>((kind, v): &'a (FinanceKind, serde_json::Value)) -> El<'a> {
    use crate::components::finance::records as r;
    let (handle, tint, title, sub) = match kind {
        FinanceKind::Cheque => {
            let note = r::cheque_note(v).map(|n| format!(" • \"{n}\"")).unwrap_or_default();
            (icons::gift(), SKY, format!("Cheque: {} {}", r::cheque_amount(v), r::cheque_token(v)), format!("Recipient: {}{note}", r::cheque_recipient(v)))
        }
        FinanceKind::Invoice => {
            let item = r::invoice_item(v).map(|i| format!(" • Item: {i}")).unwrap_or_default();
            (icons::receipt(), hex(0x10b981), format!("Invoice: {} {}", r::invoice_amount(v), r::invoice_token(v)), format!("Billed to: {}{item}", r::invoice_payer(v)))
        }
        FinanceKind::Campaign => {
            let t = v.get("title").and_then(|t| t.as_str()).unwrap_or("Crowdfund");
            (icons::flame(), hex(0xfb923c), format!("Campaign: {t}"), format!("{} · people back it from the card", v.get("chain_name").and_then(|c| c.as_str()).unwrap_or("")))
        }
        FinanceKind::Drop => {
            let token = v.get("token").and_then(|t| t.as_str()).unwrap_or("");
            let t = v.get("title").and_then(|t| t.as_str()).map(str::to_string).unwrap_or_else(|| format!("{token} drop"));
            (icons::send(), INDIGO_SOFT, format!("Drop: {t}"), format!("{} · each listed wallet claims from the card", v.get("chain").and_then(|c| c.as_str()).unwrap_or("")))
        }
    };
    container(
        row![
            container(center(icon(handle, 18, tint))).width(40).height(40).style(boxed(Color { a: 0.15, ..tint }, Color::TRANSPARENT, 6.0)),
            iced::widget::column![
                text(title).size(13).font(SEMIBOLD).color(Color::WHITE),
                text(crate::components::ui::truncate(&sub, 90)).size(11.5).color(MUTED).wrapping(iced::widget::text::Wrapping::None),
            ]
            .spacing(2)
            .width(Fill),
            button(icon(icons::x(), 14, MUTED)).padding(3).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(Message::Chat(ChatMsg::ClearAttachment)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([8, 12])
    .style(|_| container::Style {
        background: Some(rgba(0xffffff, 0.03).into()),
        border: iced::Border { color: BORDER, width: 1.0, radius: iced::border::Radius { top_left: 10.0, top_right: 10.0, bottom_right: 0.0, bottom_left: 0.0 } },
        ..Default::default()
    })
    .into()
}

#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    room: &'a ChatRoomItem,
    draft: &'a str,
    sending: bool,
    locked: Option<&'a KeyProblem>,
    mode: Mode<'a>,
    pending: Option<&'a crate::media::PendingFile>,
    thumb: Option<&'a iced::widget::image::Handle>,
    finance: Option<&'a (FinanceKind, serde_json::Value)>,
    recording: Option<&'a Recording>,
    can_record: bool,
) -> El<'a> {
    let frame = |content: El<'a>| -> El<'a> {
        container(container(content).padding([4, 8]).width(Fill).style(boxed(hex(0x15161c), BORDER, 10.0)))
            .padding(Padding { top: 4.0, right: 20.0, bottom: 18.0, left: 20.0 })
            .into()
    };
    let framed = |top: Option<El<'a>>, content: El<'a>| -> El<'a> {
        match top {
            None => frame(content),
            Some(top) => container(iced::widget::column![top, container(content).padding([4, 8]).width(Fill).style(|_| container::Style {
                background: Some(hex(0x15161c).into()),
                border: iced::Border { color: BORDER, width: 1.0, radius: iced::border::Radius { top_left: 0.0, top_right: 0.0, bottom_right: 10.0, bottom_left: 10.0 } },
                ..Default::default()
            })])
            .padding(Padding { top: 4.0, right: 20.0, bottom: 18.0, left: 20.0 })
            .into(),
        }
    };

    if let Some(problem) = locked.filter(|_| display::is_encrypted_room(room)) {
        return frame(
            row![
                icon(icons::lock(), 15, AMBER_TEXT),
                text(problem.explain()).size(12.5).color(TEXT_SECONDARY),
            ]
            .spacing(10)
            .padding([10, 6])
            .align_y(Alignment::Center)
            .into(),
        );
    }

    if room.is_member == Some(false) {
        return frame(
            row![
                text("You're reading this public room. Join it to write here.").size(13).color(TEXT_SECONDARY).width(Fill),
                button(text("Join").size(13).font(SEMIBOLD)).padding([7, 18]).style(primary).on_press(Message::Chat(ChatMsg::Join(room.room_key.clone()))),
            ]
            .spacing(10)
            .padding([4, 6])
            .align_y(Alignment::Center)
            .into(),
        );
    }

    // Recording a voice message: its red bar stands in for the box (AudioRecorderBar.tsx).
    if let Some(r) = recording.filter(|r| !r.note()) {
        return frame(super::recorder_ui::bar(r));
    }
    let attached = pending.map(|f| pending_strip(f, thumb)).or_else(|| finance.map(finance_strip));
    let top = match (strip(&mode), attached) {
        (Some(a), Some(b)) => Some(iced::widget::column![a, b].into()),
        (a, b) => a.or(b),
    };
    let can_send = (!draft.trim().is_empty() || pending.is_some() || finance.is_some()) && !sending;
    // Nothing written and nothing attached: the mic and the camera take the send button's place
    // (ChatPage.tsx), so a voice message or a video note starts without opening a menu.
    let empty = draft.trim().is_empty() && pending.is_none() && finance.is_none();
    let send: El<'a> = if empty && can_record {
        let record = |handle: svg::Handle, tint: Color, label: &'static str, on: Message| -> El<'a> {
            iced::widget::tooltip(
                button(center(icon(handle, 18, tint)))
                    .width(34)
                    .height(34)
                    .padding(0)
                    .style(move |_, status| button::Style {
                        background: Some(if matches!(status, button::Status::Hovered) { Color { a: 0.28, ..tint } } else { Color { a: 0.15, ..tint } }.into()),
                        border: iced::Border { color: Color { a: 0.3, ..tint }, width: 1.0, radius: 10.0.into() },
                        ..Default::default()
                    })
                    .on_press(on),
                container(text(label).size(11).color(TEXT_BODY)).padding([3, 7]).style(boxed(hex(0x1c1d22), BORDER, 6.0)),
                iced::widget::tooltip::Position::Top,
            )
            .into()
        };
        row![
            record(icons::mic(), INDIGO_SOFT, "Voice message", Message::Chat(ChatMsg::RecordStart(crate::recorder::Kind::Voice))),
            record(icons::video(), SKY, "Video message", Message::Chat(ChatMsg::RecordStart(crate::recorder::Kind::Note))),
        ]
        .spacing(6)
        .into()
    } else {
        button(center(icon(icons::send(), 16, if can_send { Color::WHITE } else { FAINT })))
            .width(34)
            .height(34)
            .padding(0)
            .style(move |t, s| if can_send { primary(t, s) } else { ghost(8.0, rgba(0xffffff, 0.06))(t, s) })
            .on_press_maybe(can_send.then_some(Message::Chat(ChatMsg::Send)))
            .into()
    };
    let tool = |handle: svg::Handle, on: Message| -> El<'a> {
        button(center(icon(handle, 18, MUTED))).width(32).height(32).padding(0).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(on).into()
    };
    let attach = || Message::Picker(crate::components::file_picker::PickerMsg::Open(crate::components::file_picker::Purpose::Attachment));
    framed(
        top,
        row![
            tool(icons::plus(), Message::Chat(ChatMsg::OpenPlus)),
            text_input(&if matches!(mode, Mode::Edit) { "Edit message".to_string() } else { format!("Message {}", room.title) }, draft)
                .id(COMPOSER)
                .on_input(|t| Message::Chat(ChatMsg::Draft(t)))
                .on_submit(Message::Chat(ChatMsg::Send))
                .size(14)
                .padding([10, 4])
                .style(bare_input),
            tool(icons::paperclip(), attach()),
            tool(icons::smile(), Message::Chat(ChatMsg::OpenEmoji)),
            tool(icons::sticker(), Message::Chat(ChatMsg::OpenGifs)),
            send,
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into(),
    )
}
