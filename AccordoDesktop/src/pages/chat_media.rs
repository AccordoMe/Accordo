// SPDX-License-Identifier: GPL-3.0-or-later
//! Pictures, files, GIFs and stickers in the chat (ChatPage.tsx's file attach / handleSendMedia,
//! GifPicker.tsx): what to load and decode, sending them (sealed with the room key in private
//! rooms — the whole attachment, like the web), saving, and the pop-up pickers.

use std::path::PathBuf;
use std::sync::Arc;

use iced::Task;
use serde_json::json;

use super::chat_page::{room_keys, ChatMsg, State, COMPOSER};
use crate::api::chat::{self, ChatMessageItem, SendMessage};
use crate::api::client::{Api, ApiError};
use crate::api::gifs::{self, GifAttachment, GifItem, Kind, GIF_MEDIA_PATH};
use crate::api::stickers::{self, StickerAttachment, StickerPack, STICKER_PATH};
use crate::app::Message;
use crate::components::chat::display;
use crate::components::chat::media_picker::{still_key, GifPanel, GifTab, Panel, GIF_SEARCH};
use crate::e2ee::rooms;
use crate::media::{self, Frames, PendingFile};

/// Largest side a picture is decoded at: in the feed, in the full view, GIFs, stickers, tiles.
const FEED_PX: u32 = 760;
const FULL_PX: u32 = 1800;
const GIF_PX: u32 = 360;
const STICKER_PX: u32 = 200;
const TILE_PX: u32 = 200;

#[derive(Debug, Clone)]
pub enum MediaResult {
    Ready(String, Result<Arc<Frames>, String>),
    Attached(Result<PendingFile, String>),
    Saved(Result<PathBuf, String>),
    GifStatus(bool),
    Gifs(GifTab, String, i64, Result<Vec<GifItem>, ApiError>),
    Packs(Result<Vec<StickerPack>, ApiError>),
}

fn done(r: MediaResult) -> Message {
    Message::Chat(ChatMsg::Media(Box::new(r)))
}

fn ours(path: &str) -> bool {
    path.starts_with(GIF_MEDIA_PATH) || path.starts_with(STICKER_PATH)
}

async fn fetch(api: Api, path: String, max: u32, still: bool) -> Result<Arc<Frames>, String> {
    let bytes = api.get_bytes(&path).await.map_err(|e| e.message)?;
    media::decode_async(bytes, max, still).await
}

/// Starts loading what's on screen and not decoded yet; notes whether anything is animated.
pub fn want(state: &mut State, api: &Api) -> Task<Message> {
    let mut jobs: Vec<Task<Message>> = Vec::new();
    let mut visible: Vec<String> = Vec::new();
    let mut want_one = |state: &mut State, key: String, job: Box<dyn FnOnce() -> Task<Message>>| {
        visible.push(key.clone());
        if state.media.begin(&key) {
            jobs.push(job());
        }
    };

    let mut shown: Vec<ChatMessageItem> = Vec::new();
    if let Some(feed) = state.selected.as_ref().and_then(|k| state.messages.get(k)) {
        shown.extend(feed.iter().rev().take(60).cloned());
    }
    if let Some(t) = &state.thread {
        shown.extend(t.root.iter().cloned());
        shown.extend(t.replies.iter().cloned());
    }
    for m in shown {
        let Some(key) = display::media_key(&m) else { continue };
        let a = m.attachment_data.clone().unwrap_or_default();
        match m.message_type.as_str() {
            "image" => {
                let data = a.get("file_data").and_then(|d| d.as_str()).unwrap_or_default().to_string();
                want_one(state, key.clone(), Box::new(move || {
                    let k = key.clone();
                    Task::perform(
                        async move {
                            let (_, bytes) = media::parse_data_url(&data).ok_or("Not a picture")?;
                            media::decode_async(bytes, FEED_PX, false).await
                        },
                        move |r| done(MediaResult::Ready(k.clone(), r)),
                    )
                }));
            }
            "gif" | "sticker" if ours(&key) => {
                let max = if m.message_type == "gif" { GIF_PX } else { STICKER_PX };
                let api = api.clone();
                want_one(state, key.clone(), Box::new(move || {
                    let k = key.clone();
                    Task::perform(fetch(api, key, max, false), move |r| done(MediaResult::Ready(k.clone(), r)))
                }));
            }
            _ => {}
        }
    }

    // The full view.
    if let Some(id) = state.lightbox {
        let data = crate::pages::chat_actions::find(state, id)
            .and_then(|m| m.attachment_data.as_ref())
            .and_then(|a| a.get("file_data"))
            .and_then(|d| d.as_str())
            .map(str::to_string);
        if let Some(data) = data {
            let key = format!("big:{id}");
            want_one(state, key.clone(), Box::new(move || {
                let k = key.clone();
                Task::perform(
                    async move {
                        let (_, bytes) = media::parse_data_url(&data).ok_or("Not a picture")?;
                        media::decode_async(bytes, FULL_PX, false).await
                    },
                    move |r| done(MediaResult::Ready(k.clone(), r)),
                )
            }));
        }
    }

    // Picker tiles: first frames only.
    let mut tiles: Vec<(String, u32)> = Vec::new();
    if let Some(Panel::Gifs(p)) = &state.panel {
        if let Some(items) = &p.items {
            tiles.extend(items.iter().map(|g| (g.preview.clone(), TILE_PX)));
        }
        if p.tab == GifTab::Stickers {
            for pack in p.packs.iter().flatten() {
                tiles.extend(pack.stickers.iter().map(|s| (s.url.clone(), 120)));
            }
        }
    }
    for (path, max) in tiles.into_iter().filter(|(p, _)| ours(p)) {
        let api = api.clone();
        let key = still_key(&path);
        want_one(state, key.clone(), Box::new(move || {
            let k = key.clone();
            Task::perform(fetch(api, path, max, true), move |r| done(MediaResult::Ready(k.clone(), r)))
        }));
    }

    // The thumbnail of the file waiting to be sent.
    if let Some(f) = state.pending_file.as_ref().filter(|f| f.is_image) {
        let data = f.data_url.clone();
        want_one(state, "pending".into(), Box::new(move || {
            Task::perform(
                async move {
                    let (_, bytes) = media::parse_data_url(&data).ok_or("Not a picture")?;
                    media::decode_async(bytes, 120, true).await
                },
                |r| done(MediaResult::Ready("pending".into(), r)),
            )
        }));
    }

    // Cheque cards: the server's copy and the claim history, once per session.
    let mut cheque_ids: Vec<String> = Vec::new();
    let mut cheque_live = false;
    if let Some(feed) = state.selected.as_ref().and_then(|k| state.messages.get(k)) {
        for m in feed.iter().rev().take(60).filter(|m| m.message_type == "cheque") {
            let Some(data) = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none()) else { continue };
            if let Some(id) = crate::components::chat::cheque_card::cheque_id(data) {
                let live = state.cheques.get(&id).and_then(|v| v.live.as_ref());
                cheque_live |= crate::components::chat::cheque_card::is_live(&crate::components::chat::cheque_card::merged(data, live));
                cheque_ids.push(id);
            }
        }
    }
    for id in cheque_ids {
        if !state.cheques.contains_key(&id) {
            state.cheques.insert(id.clone(), Default::default());
            let k = id.clone();
            jobs.push(Task::perform(crate::api::cheques::get(api.clone(), id.clone()), move |r| {
                Message::Chat(ChatMsg::Cheque(crate::components::chat::cheque_card::ChequeMsg::Loaded(k.clone(), r.map_err(|e| e.message))))
            }));
            jobs.push(load_cheque_claims(state, api, &id));
        }
    }
    // Campaign and drop cards: loaded once per session (Refresh on the card loads again).
    let mut fund: Vec<(bool, String)> = Vec::new();
    if let Some(feed) = state.selected.as_ref().and_then(|k| state.messages.get(k)) {
        for m in feed.iter().rev().take(60).filter(|m| matches!(m.message_type.as_str(), "campaign" | "drop")) {
            let Some(data) = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none()) else { continue };
            if m.message_type == "campaign" {
                if let Some(id) = crate::components::chat::fund_cards::campaign_id(data) {
                    fund.push((true, id));
                }
            } else if let Some(id) = crate::components::chat::fund_cards::drop_id(data) {
                fund.push((false, id));
            }
        }
    }
    // Not for a section this build lacks or the server closed: the card just says so.
    use crate::modules::Module;
    let (campaigns_on, drops_on) = (state.modules.available(Module::Crowdfund), state.modules.available(Module::Drops));
    #[cfg(feature = "drops")]
    let drop_ids: Vec<String> = fund.iter().filter(|(c, _)| !c).map(|(_, id)| id.clone()).collect();
    for (is_campaign, id) in fund {
        if (is_campaign && !campaigns_on) || (!is_campaign && !drops_on) {
            continue;
        }
        if is_campaign && !state.fund.campaigns.contains_key(&id) {
            jobs.push(load_campaign_card(state, api, &id));
        } else if !is_campaign && !state.fund.drops.contains_key(&id) {
            jobs.push(load_drop_card(state, api, &id));
        }
    }
    // Invoice cards: the same, once per session.
    let mut invoice_ids: Vec<String> = Vec::new();
    if let Some(feed) = state.selected.as_ref().and_then(|k| state.messages.get(k)) {
        for m in feed.iter().rev().take(60).filter(|m| m.message_type == "invoice") {
            let Some(data) = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none()) else { continue };
            if let Some(id) = crate::components::chat::invoice_card::invoice_id(data) {
                cheque_live |= crate::components::chat::invoice_card::is_live(data);
                invoice_ids.push(id);
            }
        }
    }
    for id in invoice_ids {
        if !state.invoices.contains_key(&id) {
            jobs.push(load_invoice(state, api, &id));
        }
    }
    state.animating = state.media.any_animated(visible.iter().map(String::as_str));
    #[cfg(feature = "drops")]
    let drops_live = {
        let t = chrono::Utc::now().timestamp().max(0) as u64;
        drop_ids.iter().filter_map(|id| state.fund.drops.get(id)).any(|c| crate::components::chat::drop_card::ticking(c, t))
    };
    #[cfg(not(feature = "drops"))]
    let drops_live = false;
    state.live_times = cheque_live || drops_live || state.selected.as_ref().and_then(|k| state.messages.get(k)).is_some_and(|f| f.iter().rev().take(60).any(|m| crate::components::chat::timestamp::is_live(&m.content)));
    Task::batch(jobs)
}

/// A campaign card: the campaign as Accordo lists it (again).
pub fn load_campaign_card(state: &mut State, api: &Api, id: &str) -> Task<Message> {
    use crate::components::chat::fund_cards::FundMsg;
    state.fund.campaigns.entry(id.to_string()).or_insert(None);
    let k = id.to_string();
    Task::perform(crate::api::crowdfund::get(api.clone(), id.to_string()), move |r| Message::Chat(ChatMsg::Fund(FundMsg::Campaign(k.clone(), r.map(|d| Box::new(d.campaign)).map_err(|e| e.message)))))
}

/// A drop card: the drop and the reader's shares in it (looked up for their wallets), in one request.
pub fn load_drop_card(state: &mut State, api: &Api, id: &str) -> Task<Message> {
    use crate::components::chat::fund_cards::FundMsg;
    state.fund.drops.entry(id.to_string()).or_default().loading = true;
    let addresses: Vec<String> = state.my_wallets.iter().map(|(a, _)| a.clone()).filter(|a| a.starts_with("0x")).collect();
    let k = id.to_string();
    Task::perform(crate::api::drops::card(api.clone(), id.to_string(), addresses), move |r| Message::Chat(ChatMsg::Fund(FundMsg::Drop(k.clone(), r.map(Box::new).map_err(|e| e.message)))))
}

/// A drop card's second half: the drop and the reader's shares as the contract has them (RPC
/// reads, not Accordo). A network that isn't in the list: the card says it shows Accordo's copy.
#[cfg(feature = "drops")]
pub fn read_drop_on_chain(state: &mut State, id: &str) -> Task<Message> {
    use crate::components::chat::fund_cards::{chain_key, FundMsg};
    let Some(card) = state.fund.drops.get_mut(id) else { return Task::none() };
    let Some(Ok(d)) = card.info.as_ref() else { return Task::none() };
    let Some(chain) = state.fund.chains.get(&chain_key(&d.chain)).cloned() else {
        card.loading = false;
        card.chain = Some(Err(format!("{} isn't in your network list", d.chain)));
        return Task::none();
    };
    card.loading = true;
    let job = crate::flows::drops::card_on_chain(chain, d.contract_address.clone(), d.drop_id.clone(), card.slots.clone().unwrap_or_default());
    let k = id.to_string();
    Task::perform(job, move |r| Message::Chat(ChatMsg::Fund(FundMsg::OnChain(k.clone(), r.map(Box::new)))))
}

#[cfg(not(feature = "drops"))]
pub fn read_drop_on_chain(_state: &mut State, _id: &str) -> Task<Message> {
    Task::none()
}

/// An invoice card's server copy and its payments (again).
pub fn load_invoice(state: &mut State, api: &Api, id: &str) -> Task<Message> {
    use crate::components::chat::invoice_card::InvoiceMsg;
    let v = state.invoices.entry(id.to_string()).or_default();
    v.loading = true;
    let (a, b) = (id.to_string(), id.to_string());
    Task::batch([
        Task::perform(crate::api::invoices::get(api.clone(), id.to_string()), move |r| Message::Chat(ChatMsg::Invoice(InvoiceMsg::Loaded(a.clone(), r.map_err(|e| e.message))))),
        Task::perform(crate::api::invoices::payments(api.clone(), id.to_string()), move |r| Message::Chat(ChatMsg::Invoice(InvoiceMsg::Payments(b.clone(), r.map_err(|e| e.message))))),
    ])
}

/// A cheque card's server copy and its claims, again (after claiming it from here).
pub fn reload_cheque(state: &mut State, api: &Api, id: &str) -> Task<Message> {
    state.cheques.entry(id.to_string()).or_default();
    let k = id.to_string();
    let live = Task::perform(crate::api::cheques::get(api.clone(), id.to_string()), move |r| {
        Message::Chat(ChatMsg::Cheque(crate::components::chat::cheque_card::ChequeMsg::Loaded(k.clone(), r.map_err(|e| e.message))))
    });
    Task::batch([live, load_cheque_claims(state, api, id)])
}

/// A cheque's claims and refunds (again: the refresh button).
pub fn load_cheque_claims(state: &mut State, api: &Api, id: &str) -> Task<Message> {
    let v = state.cheques.entry(id.to_string()).or_default();
    v.loading_claims = true;
    let k = id.to_string();
    Task::perform(crate::api::cheques::claims(api.clone(), id.to_string()), move |r| {
        Message::Chat(ChatMsg::Cheque(crate::components::chat::cheque_card::ChequeMsg::Claims(k.clone(), r.map_err(|e| e.message))))
    })
}

pub fn attach(state: &mut State, path: PathBuf) -> Task<Message> {
    if state.selected.is_none() {
        return Task::none();
    }
    state.attaching = true;
    Task::perform(media::read_attachment(path), |r| done(MediaResult::Attached(r)))
}

pub fn clear_attachment(state: &mut State) {
    state.pending_file = None;
    state.pending_finance = None;
    state.media.forget("pending");
}

/// The attachment of the message being sent: the file as the web sends it, sealed in private rooms.
pub fn file_payload(state: &mut State, room_key: &str) -> Result<Option<(String, serde_json::Value)>, String> {
    let Some(f) = state.pending_file.clone() else { return Ok(None) };
    let data = json!({ "file_name": f.name, "file_size": f.size, "file_type": f.mime, "file_data": f.data_url });
    Ok(Some((f.message_type().to_string(), seal(state, room_key, data)?)))
}

/// The picked cheque / invoice: its record as the attachment (sealed in private rooms), as the
/// web sends it.
pub fn finance_payload(state: &mut State, room_key: &str) -> Result<Option<(String, serde_json::Value)>, String> {
    let Some((kind, v)) = state.pending_finance.clone() else { return Ok(None) };
    Ok(Some((kind.message_type().to_string(), seal(state, room_key, v)?)))
}

pub fn seal(state: &mut State, room_key: &str, data: serde_json::Value) -> Result<serde_json::Value, String> {
    if !state.rooms.get(room_key).is_some_and(display::is_encrypted_room) {
        return Ok(data);
    }
    let keys = room_keys(state, room_key).ok_or("Can't send: this chat's key isn't on this computer.")?;
    rooms::seal_json(&data, &keys).ok_or_else(|| "Can't send: encrypting failed.".into())
}

/// A GIF or sticker: no text, the attachment sealed in private rooms (handleSendMedia).
pub fn send_media(state: &mut State, api: &Api, kind: &str, data: serde_json::Value) -> Result<Task<Message>, String> {
    let Some(room_id) = state.selected.clone() else { return Ok(Task::none()) };
    let sealed = seal(state, &room_id, data)?;
    state.panel = None;
    let body = SendMessage { room_id: Some(room_id), content: Some(String::new()), message_type: Some(kind.into()), attachment_data: Some(sealed), ..Default::default() };
    Ok(Task::perform(chat::send_message(api.clone(), body), |r| Message::Chat(ChatMsg::Sent(r.map(Box::new)))))
}

pub fn send_gif(state: &mut State, api: &Api, g: &GifItem) -> Result<Task<Message>, String> {
    let att = GifAttachment::from(g);
    send_media(state, api, "gif", serde_json::to_value(att).unwrap_or_default())
}

pub fn send_sticker(state: &mut State, api: &Api, s: &StickerAttachment) -> Result<Task<Message>, String> {
    send_media(state, api, "sticker", serde_json::to_value(s).unwrap_or_default())
}

/// A file or picture of a message into Downloads.
pub fn save(state: &mut State, id: i64) -> Task<Message> {
    state.menu = None;
    let Some(m) = crate::pages::chat_actions::find(state, id) else { return Task::none() };
    let Some(a) = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none()) else {
        state.notices.push("This file can't be opened on this computer.".into());
        return Task::none();
    };
    let name = a.get("file_name").and_then(|v| v.as_str()).unwrap_or("file").to_string();
    let Some((_, bytes)) = a.get("file_data").and_then(|v| v.as_str()).and_then(media::parse_data_url) else {
        state.notices.push("Nothing to save in this message.".into());
        return Task::none();
    };
    Task::perform(media::save_to_downloads(name, bytes), |r| done(MediaResult::Saved(r)))
}

// ── the pickers ──

pub fn open_emoji(state: &mut State) {
    state.panel = match state.panel {
        Some(Panel::Emoji) => None,
        _ => Some(Panel::Emoji),
    };
}

pub fn open_gifs(state: &mut State, api: &Api) -> Task<Message> {
    if matches!(state.panel, Some(Panel::Gifs(_))) {
        state.panel = None;
        return Task::none();
    }
    state.panel = Some(Panel::Gifs(GifPanel::new()));
    Task::batch([
        Task::perform(gifs::status(api.clone()), |s| done(MediaResult::GifStatus(s.enabled))),
        Task::perform(stickers::packs(api.clone()), |r| done(MediaResult::Packs(r))),
        iced::widget::operation::focus(GIF_SEARCH),
    ])
}

/// Trending, or results for the query, for the open tab (from `offset`).
pub fn load_gifs(state: &mut State, api: &Api, offset: i64) -> Task<Message> {
    let Some(Panel::Gifs(p)) = state.panel.as_mut() else { return Task::none() };
    if p.enabled == Some(false) {
        p.items = Some(vec![]);
        return Task::none();
    }
    if offset == 0 {
        p.items = None;
        p.exhausted = false;
    } else if p.loading_more || p.exhausted {
        return Task::none(); // a page is on its way, or there are no more
    } else {
        p.loading_more = true;
    }
    let (tab, query) = (p.tab, p.query.trim().to_string());
    let kind = if tab == GifTab::Gifs { Kind::Gifs } else { Kind::Stickers };
    let (api, q) = (api.clone(), query.clone());
    let job = async move {
        if q.is_empty() { gifs::trending(api, offset, kind).await } else { gifs::search(api, q, offset, kind).await }
    };
    Task::perform(job, move |r| done(MediaResult::Gifs(tab, query.clone(), offset, r)))
}


pub fn on_result(state: &mut State, result: MediaResult, api: &Api) -> (Task<Message>, Option<String>) {
    let mut error = None;
    let task = match result {
        MediaResult::Ready(key, r) => {
            if let Err(e) = &r {
                tracing::debug!("media: {key}: {e}");
            }
            state.media.finish(key, r);
            Task::none()
        }
        MediaResult::Attached(r) => {
            state.attaching = false;
            match r {
                Ok(f) => {
                    state.media.forget("pending");
                    state.pending_finance = None;
                    state.pending_file = Some(f);
                    return (iced::widget::operation::focus(COMPOSER), None);
                }
                Err(e) => error = Some(e),
            }
            Task::none()
        }
        MediaResult::Saved(r) => {
            match r {
                Ok(path) => state.notices.push(format!("Saved to {}", path.display())),
                Err(e) => error = Some(format!("Couldn't save: {e}")),
            }
            Task::none()
        }
        MediaResult::GifStatus(enabled) => {
            if let Some(Panel::Gifs(p)) = state.panel.as_mut() {
                p.enabled = Some(enabled);
                if !enabled {
                    p.tab = GifTab::Stickers;
                }
            }
            load_gifs(state, api, 0)
        }
        MediaResult::Packs(r) => {
            if let Some(Panel::Gifs(p)) = state.panel.as_mut() {
                p.packs = Some(r.unwrap_or_default());
            }
            Task::none()
        }
        MediaResult::Gifs(tab, query, offset, r) => {
            if let Some(Panel::Gifs(p)) = state.panel.as_mut() {
                if p.tab == tab && p.query.trim() == query {
                    p.loading_more = false;
                    match r {
                        Ok(items) => {
                            p.exhausted = items.is_empty();
                            match (&mut p.items, offset) {
                                (Some(list), o) if o > 0 => {
                                    // Providers repeat items across pages now and then.
                                    let seen: std::collections::HashSet<String> = list.iter().map(|g| g.id.clone()).collect();
                                    list.extend(items.into_iter().filter(|g| !seen.contains(&g.id)));
                                }
                                _ => p.items = Some(items),
                            }
                            // The first page alone is a short panel: fetch the second right away.
                            if offset == 0 && !p.exhausted {
                                let next = p.items.as_ref().map_or(0, |i| i.len() as i64);
                                return (load_gifs(state, api, next), None);
                            }
                        }
                        Err(e) => {
                            p.items.get_or_insert_with(Vec::new);
                            p.exhausted = true;
                            error = Some(format!("Couldn't load GIFs: {}", e.message));
                        }
                    }
                }
            }
            Task::none()
        }
    };
    (task, error)
}
