// SPDX-License-Identifier: GPL-3.0-or-later
//! The chat (ChatPage.tsx + notifications/ChatSync.tsx): the room list on the left, the open
//! conversation on the right — from the API, kept live by the socket.
//!
//! DMs and groups are end-to-end encrypted: their messages are opened and sealed here with the
//! room's key (e2ee::rooms), never sent as plaintext. Without the key (no chat key on this
//! computer, nobody shared the room's key yet) they show a lock line and writing is off.

use std::collections::{HashMap, HashSet};

use iced::alignment::Horizontal;
use iced::widget::{button, center, column, row, space, text};
use iced::{Alignment, Color, Fill, Task};
use serde_json::Value;

use crate::api::chat::public_rooms::Catalog;
use crate::api::chat::{self, ChatMessageItem, ChatRoomItem, CreateRoom, MemberAuthEntry, MemberAuthPatch, MessageQuery, RoomUnreadInfo, SendMessage, UnreadsResponse, UserInfo};
use crate::api::client::{Api, ApiError};
use crate::app::Message;
use crate::components::chat::display::{self, RoomKind};
use crate::components::chat::manage_room::{self, ManageMsg, Tab};
use crate::components::chat::new_chat_modal::{self, Kind, NewChatMsg};
use super::chat_actions::{self, ActionResult};
use super::room_admin::{self, AdminResult};
use crate::components::chat::message_list::{Feed, MenuKind, Rights};
use crate::components::chat::search_panel;
use crate::components::chat::thread_panel::{self, Thread};
use crate::components::chat::members_sidebar::{self, Paged};
use crate::components::chat::pinned_modal;
use crate::components::chat::media_picker::{self, GifTab, Panel};
use super::chat_media::{self, MediaResult};
use crate::components::chat::user_profile::{self, ProfileMsg};
use crate::api::chat::UserRelationshipStats;
use crate::api::chat::MembersPage;
use crate::components::chat::{browse_rooms, chat_header, chat_sidebar, composer, message_list};
use crate::e2ee::crypto::{generate_room_key, same_public_key, DeviceKeypair};
use crate::e2ee::rooms::{self, KeyProblem, RoomKeys};
use crate::components::ui::{hline, icon, El};
use crate::icons;
use crate::store::rooms_store::RoomsStore;
use crate::theme::*;

pub const COMPOSER: &str = "composer";
/// Messages loaded when a room opens.
const PAGE: i64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Unread,
    Dms,
    Groups,
}

#[derive(Debug, Clone)]
pub enum ChatMsg {
    Filter(Filter),
    OpenRoom(String),
    CloseRoom,
    Draft(String),
    Send,
    Join(String),
    // the public-room catalog (BrowsePublicRoomsModal.tsx)
    OpenBrowse,
    CloseBrowse,
    BrowseQuery(String),
    BrowseSearch,
    BrowseLoaded(Result<Catalog, ApiError>),
    RoomsLoaded(Result<Vec<ChatRoomItem>, ApiError>),
    UnreadsLoaded(Result<UnreadsResponse, ApiError>),
    MessagesLoaded(String, Result<Vec<ChatMessageItem>, ApiError>),
    Sent(Result<Box<ChatMessageItem>, ApiError>),
    Joined(String, Result<(), ApiError>),
    /// A fire-and-forget call finished (mark as read).
    Done,
    // "Create New Room" (NewChatModal.tsx)
    OpenNewChat(Kind),
    NewChat(NewChatMsg),
    NewChatProfiles(Vec<UserInfo>),
    NewChatCreated(Result<Box<ChatRoomItem>, String>),
    /// The public key of someone who shared a room key but isn't in the room's member list.
    SharerKey(i64, Result<Box<UserInfo>, ApiError>),
    // "Manage Room" (ManageRoomModal.tsx), worked by room_admin
    OpenManage(Tab),
    Manage(ManageMsg),
    Admin(Box<AdminResult>),
    // a message (hover bar, right-click menu) — chat_actions
    Menu(i64, MenuKind),
    CloseMenu,
    React(i64, String),
    Reply(i64),
    CancelReply,
    Edit(i64),
    CancelEdit,
    Delete(i64),
    TogglePin(i64),
    CopyText(i64),
    CopyId(i64),
    OpenThread(i64),
    CloseThread,
    ThreadDraft(String),
    ThreadSend,
    Action(Box<ActionResult>),
    /// Esc: close a menu, stop editing / replying.
    Escape,
    // members (ChatMembersSidebar.tsx) and pins (PinnedMessagesModal.tsx)
    ToggleMembers,
    MemberSearch(String),
    MemberMenu(i64),
    CloseMemberMenu,
    Mention(String),
    MessageUser(i64, String),
    CopyValue(String, &'static str),
    MemberPermissions(i64),
    KickMember(i64, String),
    MoreMembers,
    MembersLoaded(String, String, Result<MembersPage, ApiError>),
    OpenPinned,
    ClosePinned,
    JumpTo(i64),
    /// Searching this chat on this computer (chat_search).
    Search(super::chat_search::SearchMsg),
    Unflash(i64),
    // someone's card (UserProfileModal.tsx)
    OpenProfile(Box<UserInfo>),
    Profile(ProfileMsg),
    ProfileLoaded(i64, Result<Box<UserInfo>, ApiError>),
    ProfileRelationship(i64, Result<UserRelationshipStats, ApiError>),
    // pictures, files, GIFs, stickers — chat_media
    AttachPath(std::path::PathBuf),
    /// From the attach window: this cheque / invoice goes with the next message.
    AttachFinance(crate::components::chat::attach_picker::FinanceKind, Box<Value>),
    ClearAttachment,
    SaveAttachment(i64),
    OpenImage(i64),
    CloseImage,
    OpenEmoji,
    OpenGifs,
    ClosePanel,
    InsertEmoji(String),
    GifTab(GifTab),
    GifQuery(String),
    GifSearch,
    GifCategory(String),
    /// The picker scrolled, not near its end (nothing to do).
    GifScrolled,
    MoreGifs,
    SendGif(Box<crate::api::gifs::GifItem>),
    SendSticker(Box<crate::api::stickers::StickerAttachment>),
    Media(Box<MediaResult>),
    /// Animation clock.
    Tick,
    OpenPlus,
    /// A `<t:…>` token from the time window, into the message box.
    InsertTime(String),
    EmojiCategory(usize),
    EmojiQuery(String),
    /// Next skin tone.
    EmojiSkin,
    Invoice(crate::components::chat::invoice_card::InvoiceMsg),
    Fund(crate::components::chat::fund_cards::FundMsg),
    Cheque(crate::components::chat::cheque_card::ChequeMsg),
    /// Who's typing: drop the ones that stopped (1 s clock while anyone is).
    TypingTick,
    /// ▶ / ■ on a voice message or video note.
    Play(i64),
    /// The player of that message: a frame, or it ended.
    Player(i64, crate::player::Event),
    /// The mic / the camera in the message box: start recording.
    RecordStart(crate::recorder::Kind),
    /// The recorder's own stream: the loudness or the picture, and then the recording.
    Recording(crate::recorder::Event),
    /// Finish the recording: it goes out as the message.
    RecordStop,
    /// Throw the recording away (Esc, the trash).
    RecordCancel,
    /// Polls, quizzes and random draws (chat_polls).
    Poll(crate::components::chat::poll_card::PollMsg),
    /// A group's card (not joined): ask to join, take it back.
    Teaser(crate::components::chat::group_teaser::TeaserMsg),
    /// "Join with a link" (an invite link, maybe approval-only).
    JoinInvite(crate::components::chat::join_invite::JoinMsg),
}

/// The feed's scrollable (jumping to a message).
pub const FEED: &str = "feed";

/// The live bars a voice recording's bar draws (the web's).
pub const LIVE_BARS: usize = 24;

/// A recording that's running — the mic's red bar or the camera's round window: what they show,
/// and the way to stop it. Dropping it stops the recorder (its process with it).
pub struct Recording {
    pub kind: crate::recorder::Kind,
    pub started: std::time::Instant,
    /// The loudness now, 0–100 (a voice message's last bar).
    pub level: f32,
    /// The last levels: the bars the red bar draws, oldest first.
    pub bars: Vec<f32>,
    /// A video note's latest picture (mirrored, as the web's preview).
    pub frame: Option<iced::widget::image::Handle>,
    /// The file being written here; a cancelled recording has it deleted.
    pub file: std::path::PathBuf,
    _stop: iced::task::Handle,
}

impl Recording {
    /// Seconds recorded so far.
    pub fn elapsed(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    /// The longest this recording may be (seconds): the web's.
    pub fn limit(&self) -> u64 {
        match self.kind {
            crate::recorder::Kind::Voice => crate::recorder::MAX_VOICE_SECS,
            crate::recorder::Kind::Note => crate::recorder::MAX_NOTE_SECS,
        }
    }

    /// A video note (else a voice message).
    pub fn note(&self) -> bool {
        self.kind == crate::recorder::Kind::Note
    }
}

pub struct State {
    pub me_id: i64,
    pub rooms: RoomsStore,
    pub unreads: HashMap<String, RoomUnreadInfo>,
    /// Loaded feeds, by room key (oldest first).
    pub messages: HashMap<String, Vec<ChatMessageItem>>,
    pub loading: HashSet<String>,
    pub selected: Option<String>,
    pub filter: Filter,
    pub draft: String,
    pub sending: bool,
    /// The public-room catalog, when open.
    pub browse: Option<Browse>,
    /// Room keys of this session (from the chat key on this computer).
    pub keys: RoomKeys,
    /// Encrypted rooms whose key isn't available here, and why.
    pub locked: HashMap<String, KeyProblem>,
    /// Sharers whose public key is being looked up.
    looking_up: HashSet<i64>,
    pub new_chat: Option<new_chat_modal::State>,
    /// The "Join with a link" window, when open.
    pub join: Option<crate::components::chat::join_invite::State>,
    /// The note on a group card's "Ask to join", and a request on its way.
    pub teaser_note: iced::widget::text_editor::Content,
    /// The new poll / quiz and random draw windows, and the polls on screen (by message id).
    pub poll_form: Option<crate::components::chat::poll_card::PollForm>,
    pub draw_form: Option<crate::components::chat::poll_card::DrawForm>,
    pub polls: HashMap<i64, crate::components::chat::poll_card::PollView>,
    pub teaser_busy: bool,
    /// The account may create public rooms (a server setting).
    pub can_create_public: bool,
    pub is_admin: bool,
    /// Which money sections are on (the + menu shows only those); kept in step by the app.
    pub modules: crate::modules::Modules,
    pub manage: Option<manage_room::State>,
    /// Members of a room who joined without a provable admission: they wait for an approval.
    pub pending_admissions: HashMap<String, Vec<i64>>,
    /// Rooms whose key is being shared right now.
    pub healing: HashSet<String>,
    /// Things done, to show as toasts (drained by the app).
    pub notices: Vec<String>,
    /// The open message menu (right-click or reaction picker).
    pub menu: Option<(i64, MenuKind)>,
    /// The message being answered.
    pub reply_to: Option<ChatMessageItem>,
    /// The message being changed (its text is in the draft) and the draft from before.
    pub editing: Option<i64>,
    pub saved_draft: String,
    pub thread: Option<Thread>,
    pub members_open: bool,
    pub member_search: String,
    pub member_menu: Option<i64>,
    /// Big rooms: members from the server, page by page.
    pub members_paged: Option<Paged>,
    pub pinned_open: bool,
    /// The search panel, open for one chat, and what's been read for searching (memory only).
    pub search: Option<super::chat_search::Search>,
    pub search_index: super::chat_search::Indexes,
    /// A message just jumped to (highlighted for a moment).
    pub flash: Option<i64>,
    pub profile: Option<user_profile::State>,
    pub media: crate::media::Cache,
    clock_start: std::time::Instant,
    /// The voice message / video note playing now (dropping it stops it).
    pub player: Option<crate::components::chat::media_cards::Playing>,
    /// The voice message / video note being recorded now (dropping it stops the recorder).
    pub recording: Option<Recording>,
    /// Who's typing, by room: (user id, name, shown until).
    pub typing: HashMap<String, Vec<(i64, String, std::time::Instant)>>,
    /// When this app last said "typing" (sent at most every TYPING_EVERY).
    typing_sent: Option<std::time::Instant>,
    /// Online / last seen, by user id, from user_presence: put on every copy of that user here
    /// (room members, the members list, an open profile).
    presence: HashMap<i64, (bool, Option<String>)>,
    pub clock: u64,
    /// Something animated is on screen (the clock ticks).
    pub animating: bool,
    pub pending_file: Option<crate::media::PendingFile>,
    /// A cheque / invoice picked in the attach window, sent as a card with the next message.
    pub pending_finance: Option<(crate::components::chat::attach_picker::FinanceKind, Value)>,
    pub attaching: bool,
    pub panel: Option<Panel>,
    /// A picture shown full size (its message).
    pub lightbox: Option<i64>,
    /// Relative times or countdowns are on screen (the clock ticks each second).
    pub live_times: bool,
    pub emoji_ui: crate::components::chat::emoji_picker::EmojiUi,
    /// Cheque cards, by cheque id.
    pub cheques: HashMap<String, crate::components::chat::cheque_card::ChequeView>,
    /// Invoice cards: the server's copy and the payments, per invoice id.
    pub invoices: HashMap<String, crate::components::chat::invoice_card::InvoiceView>,
    /// Campaign and drop cards: what they load live.
    pub fund: crate::components::chat::fund_cards::Cards,
    /// The signed-in username (an invoice's creator sees its PIN).
    pub me_name: String,
    /// Network name (lowercase) → its explorer.
    pub explorers: HashMap<String, String>,
    /// The account's wallets: (address lowercase, name).
    pub my_wallets: Vec<(String, String)>,
    /// Privacy (app settings): veil messages until hovered, hide the previews in the list.
    pub blur_messages: bool,
    pub blur_previews: bool,
}

#[derive(Default)]
pub struct Browse {
    pub query: String,
    /// None while loading.
    pub results: Option<Catalog>,
    /// A room being joined.
    pub joining: Option<String>,
}

impl State {
    /// `account`: id@server (whose key pins); "" when nobody's signed in.
    pub fn new(me_id: i64, account: String) -> Self {
        Self {
            me_id,
            rooms: RoomsStore::default(),
            unreads: HashMap::new(),
            messages: HashMap::new(),
            loading: HashSet::new(),
            selected: None,
            filter: Filter::All,
            draft: String::new(),
            sending: false,
            browse: None,
            keys: RoomKeys::new(me_id, account),
            locked: HashMap::new(),
            looking_up: HashSet::new(),
            new_chat: None,
            join: None,
            teaser_note: Default::default(),
            poll_form: None,
            draw_form: None,
            polls: HashMap::new(),
            teaser_busy: false,
            can_create_public: false,
            is_admin: false,
            modules: crate::modules::Modules::default(),
            manage: None,
            pending_admissions: HashMap::new(),
            healing: HashSet::new(),
            notices: Vec::new(),
            menu: None,
            reply_to: None,
            editing: None,
            saved_draft: String::new(),
            thread: None,
            members_open: false,
            member_search: String::new(),
            member_menu: None,
            members_paged: None,
            pinned_open: false,
            search: None,
            search_index: Default::default(),
            flash: None,
            profile: None,
            media: crate::media::Cache::default(),
            clock_start: std::time::Instant::now(),
            typing: HashMap::new(),
            typing_sent: None,
            player: None,
            recording: None,
            presence: HashMap::new(),
            clock: 0,
            animating: false,
            pending_file: None,
            pending_finance: None,
            attaching: false,
            panel: None,
            lightbox: None,
            live_times: false,
            emoji_ui: Default::default(),
            cheques: HashMap::new(),
            invoices: HashMap::new(),
            me_name: String::new(),
            fund: Default::default(),
            explorers: HashMap::new(),
            my_wallets: Vec::new(),
            blur_messages: false,
            blur_previews: false,
        }
    }

    pub fn unread(&self, room_key: &str) -> &RoomUnreadInfo {
        static NONE: RoomUnreadInfo = RoomUnreadInfo { unread_count: 0, mention_count: 0, last_read_id: 0 };
        self.unreads.get(room_key).unwrap_or(&NONE)
    }

    /// For the rail's badge.
    pub fn total_unread(&self) -> u32 {
        self.unreads.values().map(|u| u.unread_count.max(0) as u32).sum()
    }

    pub fn selected_room(&self) -> Option<&ChatRoomItem> {
        self.selected.as_deref().and_then(|k| self.rooms.get(k))
    }

    /// Rooms of `kind` shown under the current filter, newest activity first.
    pub fn visible_rooms(&self, kind: RoomKind) -> Vec<&ChatRoomItem> {
        let mut list: Vec<&ChatRoomItem> = self
            .rooms
            .rooms
            .iter()
            .filter(|r| display::kind(r) == kind)
            .filter(|r| match self.filter {
                Filter::All => true,
                Filter::Unread => self.unread(&r.room_key).unread_count > 0,
                Filter::Dms => kind == RoomKind::Dm,
                Filter::Groups => kind != RoomKind::Dm,
            })
            .collect();
        list.sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));
        list
    }

    fn push_message(&mut self, m: ChatMessageItem) {
        if let Some(feed) = self.messages.get_mut(&m.room_id) {
            match feed.iter_mut().find(|x| x.id == m.id) {
                Some(existing) => *existing = m,
                None => feed.push(m),
            }
        }
    }
}

fn msg(m: ChatMsg) -> Message {
    Message::Chat(m)
}

/// The keys of an encrypted room (current first), or None — then `state.locked` says why.
/// Public rooms: None, nothing to open.
pub fn room_keys(state: &mut State, room_key: &str) -> Option<Vec<String>> {
    let room = state.rooms.get(room_key)?;
    if !display::is_encrypted_room(room) {
        return None;
    }
    match state.keys.resolve(room) {
        Ok(keys) => {
            state.locked.remove(room_key);
            Some(keys)
        }
        Err(problem) => {
            state.locked.insert(room_key.to_string(), problem);
            None
        }
    }
}

/// Looks up a sharer we don't know the key of (they left the room since).
fn look_up_sharer(state: &mut State, api: &Api, room_key: &str) -> Task<Message> {
    match state.locked.get(room_key) {
        Some(KeyProblem::UnknownSharer(id)) if state.looking_up.insert(*id) => {
            let id = *id;
            Task::perform(chat::get_user_profile(api.clone(), id.to_string()), move |r| msg(ChatMsg::SharerKey(id, r.map(Box::new))))
        }
        _ => Task::none(),
    }
}

/// The chat list's last lines of encrypted rooms, opened where we have the key.
fn open_previews(state: &mut State) {
    let sealed: Vec<String> = state
        .rooms
        .rooms
        .iter()
        .filter(|r| display::is_encrypted_room(r) && r.last_message.as_deref().is_some_and(display::looks_encrypted))
        .map(|r| r.room_key.clone())
        .collect();
    for key in sealed {
        if let Some(keys) = room_keys(state, &key) {
            state.rooms.update_room(&key, |r| {
                if let Some(text) = r.last_message.as_deref().and_then(|t| rooms::open_preview(t, &keys)) {
                    r.last_message = Some(text);
                }
            });
        }
    }
}

/// The member list of a big room, from the server (a small room's list is on the room).
fn load_members(state: &mut State, api: &Api, offset: i64) -> Task<Message> {
    let Some(room) = state.selected_room() else { return Task::none() };
    if !state.members_open || !members_sidebar::is_paged(room) {
        state.members_paged = None;
        return Task::none();
    }
    let key = room.room_key.clone();
    let query = state.member_search.trim().to_string();
    let same = state.members_paged.as_ref().is_some_and(|p| p.room_key == key && p.query == query);
    if !same || offset == 0 {
        state.members_paged = Some(Paged { room_key: key.clone(), query: query.clone(), members: Vec::new(), total: room.member_count.unwrap_or(0), loading: true });
    } else if let Some(p) = state.members_paged.as_mut() {
        p.loading = true;
    }
    let (k, q) = (key.clone(), query.clone());
    Task::perform(chat::list_room_members(api.clone(), key, Some(query), offset, 50), move |r| msg(ChatMsg::MembersLoaded(k.clone(), q.clone(), r)))
}

/// Reads a room's messages again (its key just arrived).
pub fn reload_messages(state: &mut State, api: &Api, room_key: &str) -> Task<Message> {
    if state.messages.remove(room_key).is_some() || state.selected.as_deref() == Some(room_key) {
        return load_messages(state, api, room_key);
    }
    Task::none()
}

/// The chat key on this computer changed (loaded, restored, imported): open everything again.
pub fn set_device_key(state: &mut State, keypair: Option<DeviceKeypair>, api: &Api) -> Task<Message> {
    state.keys.set_keypair(keypair);
    state.locked.clear();
    state.looking_up.clear();
    let open = state.selected.clone();
    state.messages.clear();
    let mut tasks = vec![load(state, api)];
    if let Some(key) = open {
        tasks.push(load_messages(state, api, &key));
    }
    Task::batch(tasks)
}

/// What a message preview line says for a message (attachments by kind).
fn preview_of(m: &ChatMessageItem) -> String {
    if m.message_type != "text" && !m.message_type.is_empty() { format!("[{}]", m.message_type.to_uppercase()) } else { m.content.clone() }
}

/// Seals and sends the draft (sealed with the room key in encrypted rooms — never plaintext there).
fn send(state: &mut State, api: &Api) -> Result<Task<Message>, String> {
    if let Some(id) = state.editing {
        return chat_actions::save_edit(state, api, id);
    }
    let mut text = state.draft.trim().to_string();
    let Some(room) = state.selected_room() else { return Ok(Task::none()) };
    if (text.is_empty() && state.pending_file.is_none() && state.pending_finance.is_none()) || state.sending || state.attaching {
        return Ok(Task::none());
    }
    // A card alone still says what it is (handleSend's defaultText).
    if text.is_empty() {
        if let Some((kind, v)) = &state.pending_finance {
            text = kind.default_text(v);
        }
    }
    let room_id = room.room_key.clone();
    let content = if text.is_empty() {
        String::new()
    } else if display::is_encrypted_room(room) {
        let keys = room_keys(state, &room_id).ok_or("Can't send: this chat's key isn't on this computer.")?;
        rooms::seal_text(&text, &keys).ok_or("Can't send: encrypting the message failed.")?
    } else {
        text
    };
    let (reply_to_id, reply_to_data) = chat_actions::reply_payload(state, &room_id)?;
    let file = match chat_media::file_payload(state, &room_id)? {
        Some(f) => Some(f),
        None => chat_media::finance_payload(state, &room_id)?,
    };
    state.sending = true;
    let (kind, attachment_data) = match file {
        Some((kind, data)) => (kind, Some(data)),
        None => ("text".to_string(), None),
    };
    let body = SendMessage { room_id: Some(room_id), content: Some(content), message_type: Some(kind), attachment_data, reply_to_id, reply_to_data, ..Default::default() };
    Ok(Task::perform(chat::send_message(api.clone(), body), |r| msg(ChatMsg::Sent(r.map(Box::new)))))
}

/// Throws the recording away: the process is killed and the half-written file deleted.
fn cancel_recording(state: &mut State) {
    if let Some(r) = state.recording.take() {
        crate::recorder::cancel();
        let _ = std::fs::remove_file(&r.file);
    }
}

/// The attachment a recording carries, as the web's recorders make it: the file as a data URL,
/// its name, size and type, the length in seconds — and the waveform (voice) or `is_round` with
/// the first frame as its thumbnail (video note).
fn recording_data(rec: &crate::recorder::Recording) -> Value {
    let note = rec.kind == crate::recorder::Kind::Note;
    let mut data = serde_json::json!({
        "file_data": crate::media::data_url(rec.mime, &rec.bytes),
        "file_name": rec.file_name,
        "file_size": rec.bytes.len(),
        "file_type": rec.mime,
        "duration": rec.duration,
    });
    if note {
        data["is_round"] = serde_json::json!(true);
        if let Some(t) = &rec.thumbnail {
            data["thumbnail"] = serde_json::json!(t);
        }
    } else {
        data["waveform"] = serde_json::json!(rec.waveform);
    }
    data
}

/// Sends a finished recording as the web does — an `audio` / `video` message with the file in it,
/// the caption sealed in private rooms (so the feed and other clients say what it is).
fn send_recording(state: &mut State, api: &Api, rec: crate::recorder::Recording) -> Result<Task<Message>, String> {
    let Some(room_key) = state.selected.clone().filter(|k| state.rooms.get(k).is_some()) else {
        return Err("That conversation was closed — the recording is gone".into());
    };
    let note = rec.kind == crate::recorder::Kind::Note;
    let encrypted = state.rooms.get(&room_key).is_some_and(display::is_encrypted_room);
    let caption = if note { format!("📹 Video message ({}s)", rec.duration) } else { format!("🎙️ Voice message ({}s)", rec.duration) };
    let content = if encrypted {
        let keys = room_keys(state, &room_key).ok_or("Can't send: this chat's key isn't on this computer.")?;
        rooms::seal_text(&caption, &keys).ok_or("Can't send: encrypting the message failed.")?
    } else {
        caption
    };
    let sealed = chat_media::seal(state, &room_key, recording_data(&rec))?;
    let (reply_to_id, reply_to_data) = chat_actions::reply_payload(state, &room_key)?;
    let body = SendMessage {
        room_id: Some(room_key),
        content: Some(content),
        message_type: Some(if note { "video" } else { "audio" }.to_string()),
        attachment_data: Some(sealed),
        reply_to_id,
        reply_to_data,
        ..Default::default()
    };
    state.sending = true;
    Ok(Task::perform(chat::send_message(api.clone(), body), |r| msg(ChatMsg::Sent(r.map(Box::new)))))
}

async fn fetch_profiles(api: Api, usernames: Vec<String>) -> Vec<UserInfo> {
    let lookups = usernames.into_iter().map(|u| chat::get_user_profile(api.clone(), crate::api::client::enc(&u)));
    iced::futures::future::join_all(lookups).await.into_iter().filter_map(Result::ok).collect()
}

/// Creates the DM / group with the envelopes already made; a group also gets owner admission tags
/// for the members whose keys we vouched for (authorizeGroupMembers), so other members' apps
/// share the room key with them later.
async fn create_private(api: Api, group: bool, request: CreateRoom, room_aes_key: String, vouched: Vec<(i64, String)>) -> Result<Box<ChatRoomItem>, String> {
    if !group {
        return chat::create_dm_room(api, request).await.map(Box::new).map_err(|e| e.message);
    }
    let room = chat::create_group_room(api.clone(), request).await.map_err(|e| e.message)?;
    let mut entries = HashMap::new();
    for m in &room.allowed_users {
        let Some(public) = m.e2ee_public_key.as_deref() else { continue };
        if !vouched.iter().any(|(id, k)| *id == m.id && same_public_key(k, public)) {
            continue;
        }
        if let Some(tag) = rooms::owner_tag(&room_aes_key, &room.room_key, m.id, public) {
            entries.insert(m.id.to_string(), MemberAuthEntry { tag, kind: "owner".into() });
        }
    }
    if !entries.is_empty() {
        let patch = MemberAuthPatch { entries: Some(entries), ..Default::default() };
        if let Err(e) = chat::update_member_auth(api, room.room_key.clone(), patch).await {
            tracing::warn!("e2ee: couldn't issue admission tags for the new group: {}", e.message);
        }
    }
    Ok(Box::new(room))
}

fn submit_new_chat(state: &mut State, api: &Api) -> Task<Message> {
    let has_key = state.keys.has_device_key();
    let Some(form) = state.new_chat.as_mut() else { return Task::none() };
    if form.busy {
        return Task::none();
    }
    let topic = Some(form.topic.trim().to_string()).filter(|t| !t.is_empty());
    match form.kind {
        Kind::Public => {
            let title = form.title.trim();
            if title.is_empty() {
                form.error = Some("Please enter a name for the public chat".into());
                return Task::none();
            }
            let title = if title.starts_with('#') { title.to_string() } else { format!("# {title}") };
            form.busy = true;
            let request = CreateRoom { title: Some(title), topic, ..Default::default() };
            Task::perform(chat::create_public_room(api.clone(), request), |r| msg(ChatMsg::NewChatCreated(r.map(Box::new).map_err(|e| e.message))))
        }
        Kind::Dm | Kind::Group => {
            if !has_key {
                form.error = Some("Your chat key isn't on this computer — restore it in Settings → Chat encryption first. New chats are encrypted with it.".into());
                return Task::none();
            }
            let names = if form.kind == Kind::Dm {
                let target = form.dm_target();
                if target.is_empty() {
                    form.error = Some("Please enter a username to message".into());
                    return Task::none();
                }
                vec![target]
            } else {
                if !form.max_members.is_empty() && !matches!(form.max_members.parse::<i64>(), Ok(2..=500)) {
                    form.error = Some("Max members: between 2 and 500, or empty for unlimited".into());
                    return Task::none();
                }
                form.member_usernames()
            };
            form.busy = true;
            Task::perform(fetch_profiles(api.clone(), names), |p| msg(ChatMsg::NewChatProfiles(p)))
        }
    }
}

/// The members' profiles are in: make the room key, wrap it for everyone we trust, create.
fn create_with_profiles(state: &mut State, api: &Api, profiles: Vec<UserInfo>) -> Task<Message> {
    let Some(my_key) = state.keys.public_key().map(str::to_string) else { return Task::none() };
    let Some(form) = state.new_chat.as_ref() else { return Task::none() };
    let room_aes_key = generate_room_key();
    let mut members = vec![(state.me_id, my_key)];
    members.extend(rooms::members_with_keys(&profiles).into_iter().filter(|(id, _)| *id != state.me_id));
    let envelopes = state.keys.wrap_for(&room_aes_key, &members);
    let vouched: Vec<(i64, String)> = members.into_iter().filter(|(id, _)| envelopes.contains_key(&id.to_string())).collect();

    let settings = HashMap::from([
        ("e2ee_cipher".to_string(), Value::from(form.cipher.id())),
        ("auto_burn".to_string(), Value::from(form.auto_burn)),
        ("anti_copy".to_string(), Value::from(form.anti_copy)),
    ]);
    let group = form.kind == Kind::Group;
    let request = if group {
        let usernames = form.member_usernames();
        let title = Some(form.title.trim().to_string()).filter(|t| !t.is_empty()).or_else(|| usernames.is_empty().then(|| "🔒 Ghost Secret Room".to_string()));
        CreateRoom {
            title,
            topic: Some(form.topic.trim().to_string()).filter(|t| !t.is_empty()),
            allowed_usernames: (!usernames.is_empty()).then_some(usernames),
            max_members: form.max_members.parse().ok(),
            settings: Some(settings),
            key_envelopes: Some(envelopes),
            ..Default::default()
        }
    } else {
        CreateRoom { username: Some(form.dm_target()), settings: Some(settings), key_envelopes: Some(envelopes), ..Default::default() }
    };
    Task::perform(create_private(api.clone(), group, request, room_aes_key, vouched), |r| msg(ChatMsg::NewChatCreated(r)))
}

/// Rooms and unread counters (on sign-in, after a reconnect, when the room set changed).
pub fn load(state: &mut State, api: &Api) -> Task<Message> {
    let mut tasks = vec![Task::perform(chat::get_unreads(api.clone()), |r| msg(ChatMsg::UnreadsLoaded(r)))];
    if state.rooms.begin_load() {
        tasks.push(Task::perform(chat::get_rooms(api.clone()), |r| msg(ChatMsg::RoomsLoaded(r))));
    }
    Task::batch(tasks)
}

/// "Join with a link": join, or — on an approval-only link — the request and its note.
fn join_invite_update(state: &mut State, j: crate::components::chat::join_invite::JoinMsg, api: &Api) -> (Task<Message>, Option<String>) {
    use crate::components::chat::join_invite::{self as ji, JoinMsg};
    let jm = |j: JoinMsg| msg(ChatMsg::JoinInvite(j));
    match j {
        JoinMsg::Open => state.join = Some(ji::State::default()),
        JoinMsg::Close => state.join = None,
        JoinMsg::Link(v) => {
            if let Some(w) = state.join.as_mut() {
                w.link = v;
                w.error = None;
            }
        }
        JoinMsg::Join => {
            let Some(w) = state.join.as_mut().filter(|w| !w.busy) else { return (Task::none(), None) };
            let Some(inv) = ji::parse(&w.link) else { return (Task::none(), None) };
            w.busy = true;
            w.error = None;
            // Proof of holding the link for the members' apps (computeInviteTag); without a chat
            // key on this computer the join still goes, an admin approves the admission instead.
            let tag = inv.secret.as_deref().and_then(|s| state.keys.public_key().map(|pk| rooms::invite_tag(s, state.me_id, pk)));
            return (Task::perform(chat::join_by_invite(api.clone(), inv.code, tag), move |r| jm(JoinMsg::Joined(r.map(Box::new)))), None);
        }
        JoinMsg::Joined(Ok(room)) => {
            if room.status.as_deref() == Some("pending") {
                if let Some(w) = state.join.as_mut() {
                    w.busy = false;
                    w.requested = Some(ji::Requested { room_key: room.room_key.clone(), title: Some(room.title.clone()).filter(|t| !t.is_empty()), note: Default::default(), note_sent: false });
                }
                return (Task::none(), None);
            }
            state.join = None;
            state.notices.push(format!("Joined \"{}\"!", room.title));
            let key = room.room_key.clone();
            state.selected = Some(key.clone());
            state.messages.remove(&key);
            return (Task::batch([load(state, api), load_messages(state, api, &key)]), None);
        }
        JoinMsg::Joined(Err(e)) => {
            if let Some(w) = state.join.as_mut() {
                w.busy = false;
                let mut text = e.message;
                // A link from another server: its code means nothing here.
                let here = crate::api::client::normalize_base(api.base());
                let host = |s: &str| reqwest::Url::parse(s).ok().and_then(|u| u.host_str().map(str::to_lowercase));
                if let Some(theirs) = ji::parse(&w.link).and_then(|i| i.origin).filter(|o| e.status == Some(404) && host(o) != host(&here)) {
                    text.push_str(&format!(" — the link is for {theirs}, and you're signed in to {here}"));
                }
                w.error = Some(text);
            }
        }
        JoinMsg::Note(a) => {
            if let Some(r) = state.join.as_mut().and_then(|w| w.requested.as_mut()) {
                r.note.perform(a);
            }
        }
        JoinMsg::SendNote => {
            let Some(w) = state.join.as_mut().filter(|w| !w.busy) else { return (Task::none(), None) };
            let Some(r) = w.requested.as_ref().filter(|r| !r.note_sent) else { return (Task::none(), None) };
            let note: String = r.note.text().trim().chars().take(ji::NOTE_MAX).collect();
            if note.is_empty() {
                return (Task::none(), None);
            }
            w.busy = true;
            w.error = None;
            let key = r.room_key.clone();
            return (Task::perform(chat::add_join_request_note(api.clone(), key, note), move |r| jm(JoinMsg::Noted(r.map(|_| ()).map_err(|e| e.message)))), None);
        }
        JoinMsg::Noted(r) => {
            if let Some(w) = state.join.as_mut() {
                w.busy = false;
                match r {
                    Ok(()) => {
                        if let Some(req) = w.requested.as_mut() {
                            req.note_sent = true;
                        }
                        state.notices.push("Note sent".into());
                    }
                    Err(e) => w.error = Some(e),
                }
            }
        }
        JoinMsg::Withdraw => {
            let Some(w) = state.join.as_mut().filter(|w| !w.busy) else { return (Task::none(), None) };
            let Some(key) = w.requested.as_ref().map(|r| r.room_key.clone()) else { return (Task::none(), None) };
            w.busy = true;
            w.error = None;
            return (Task::perform(chat::withdraw_join_request(api.clone(), key), move |r| jm(JoinMsg::Withdrawn(r.map(|_| ()).map_err(|e| e.message)))), None);
        }
        JoinMsg::Withdrawn(Ok(())) => {
            state.join = None;
            state.notices.push("Request withdrawn".into());
        }
        JoinMsg::Withdrawn(Err(e)) => {
            if let Some(w) = state.join.as_mut() {
                w.busy = false;
                w.error = Some(e);
            }
        }
    }
    (Task::none(), None)
}

/// "Typing" goes out at most this often while someone types (the web: 2.5 s)…
const TYPING_EVERY: std::time::Duration = std::time::Duration::from_millis(2500);
/// …and someone else's shows this long after their last signal (the web: 3.5 s).
const TYPING_SHOWN: std::time::Duration = std::time::Duration::from_millis(3500);

/// Tells the open room we're typing (useChatMessages.notifyTyping). The server sends it to the
/// room's people only (app/room_events).
fn say_typing(state: &mut State) {
    let Some(room) = state.selected.clone() else { return };
    if state.typing_sent.is_some_and(|t| t.elapsed() < TYPING_EVERY) {
        return;
    }
    if crate::api::socket::send(serde_json::json!({ "action": "typing", "room_id": room })) {
        state.typing_sent = Some(std::time::Instant::now());
    }
}

/// Puts what user_presence said on every copy of those users here.
fn apply_presence(state: &mut State) {
    if state.presence.is_empty() {
        return;
    }
    let presence = &state.presence;
    let fix = |u: &mut UserInfo| {
        if let Some((online, seen)) = presence.get(&u.id) {
            u.is_online = Some(*online);
            if seen.is_some() {
                u.last_seen_at = seen.clone();
            }
        }
    };
    for r in state.rooms.rooms.iter_mut() {
        r.allowed_users.iter_mut().for_each(fix);
    }
    if let Some(p) = state.members_paged.as_mut() {
        p.members.iter_mut().for_each(fix);
    }
    if let Some(p) = state.profile.as_mut() {
        fix(&mut p.user);
    }
}

impl State {
    /// Someone is typing somewhere (the 1 s clock that clears them runs only then).
    pub fn anyone_typing(&self) -> bool {
        !self.typing.is_empty()
    }
}

fn load_messages(state: &mut State, api: &Api, room_key: &str) -> Task<Message> {
    // A group card: nothing to read until you're in.
    if state.rooms.get(room_key).is_some_and(display::is_teaser) {
        return Task::none();
    }
    if !state.loading.insert(room_key.to_string()) {
        return Task::none();
    }
    let key = room_key.to_string();
    let q = MessageQuery { room_id: Some(key.clone()), limit: Some(PAGE), ..Default::default() };
    Task::perform(chat::get_messages(api.clone(), q), move |r| msg(ChatMsg::MessagesLoaded(key.clone(), r)))
}

/// Marks the room read up to its newest loaded message, here and on the server.
fn mark_read(state: &mut State, api: &Api, room_key: &str) -> Task<Message> {
    let last = state.messages.get(room_key).and_then(|f| f.last()).map(|m| m.id);
    let had_unread = state.unread(room_key).unread_count > 0;
    if let Some(u) = state.unreads.get_mut(room_key) {
        u.unread_count = 0;
        u.mention_count = 0;
        if let Some(id) = last {
            u.last_read_id = id;
        }
    }
    if !had_unread && last.is_none() {
        return Task::none();
    }
    Task::perform(chat::mark_room_read(api.clone(), room_key.to_string(), last), |_| msg(ChatMsg::Done))
}

fn search_catalog(api: &Api, query: String) -> Task<Message> {
    Task::perform(chat::public_rooms::catalog(api.clone(), query, 0, 30), |r| msg(ChatMsg::BrowseLoaded(r)))
}

/// Handles a chat message. The `String` is an error to show as a toast.
pub fn update(state: &mut State, message: ChatMsg, api: &Api) -> (Task<Message>, Option<String>) {
    let tick = matches!(message, ChatMsg::Tick);
    let (task, error) = update_inner(state, message, api);
    if tick {
        return (task, error);
    }
    // Whatever is on screen now: load the pictures it shows.
    (Task::batch([task, chat_media::want(state, api), super::chat_polls::want(state, api)]), error)
}

fn update_inner(state: &mut State, message: ChatMsg, api: &Api) -> (Task<Message>, Option<String>) {
    let mut error = None;
    let task = match message {
        ChatMsg::Filter(filter) => {
            state.filter = filter;
            Task::none()
        }
        ChatMsg::OpenRoom(key) => {
            if state.selected.as_deref() != Some(key.as_str()) {
                state.member_menu = None;
                state.members_paged = None;
                state.member_search.clear();
                state.pinned_open = false;
                state.search = None;
                state.thread = None;
                state.menu = None;
                state.reply_to = None;
                chat_actions::cancel_edit(state);
                // A recording belongs to the chat it was started in: don't send it elsewhere.
                cancel_recording(state);
            }
            state.selected = Some(key.clone());
            if state.rooms.get(&key).is_some_and(display::is_teaser) {
                return (Task::none(), None);
            }
            room_keys(state, &key);
            let lookup = look_up_sharer(state, api, &key);
            let heal = room_admin::heal(state, api, &key);
            let fetch = if state.messages.contains_key(&key) { mark_read(state, api, &key) } else { load_messages(state, api, &key) };
            Task::batch([fetch, lookup, heal, iced::widget::operation::focus(COMPOSER)])
        }
        ChatMsg::CloseRoom => {
            cancel_recording(state);
            state.search = None;
            state.selected = None;
            Task::none()
        }
        ChatMsg::Draft(text) => {
            if !text.trim().is_empty() {
                say_typing(state);
            }
            state.draft = text;
            Task::none()
        }
        ChatMsg::Play(id) => {
            // The one playing: stop it. Any other: stop that, play this.
            let again = state.player.as_ref().is_some_and(|p| p.message_id == id);
            state.player = None;
            if again {
                return (Task::none(), None);
            }
            let Some(m) = chat_actions::find(state, id) else { return (Task::none(), None) };
            let Some(a) = m.attachment_data.as_ref().filter(|a| a.get("encrypted").is_none()) else {
                return (Task::none(), Some("This message can't be opened on this computer.".into()));
            };
            let mime = a.get("file_type").and_then(Value::as_str).unwrap_or("").to_string();
            let video = m.message_type == "video" || mime.starts_with("video/");
            let duration = a.get("duration").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let Some((_, bytes)) = a.get("file_data").and_then(Value::as_str).and_then(crate::media::parse_data_url) else {
                return (Task::none(), Some("Nothing to play in this message.".into()));
            };
            let file = match crate::player::write_temp(id, &bytes, crate::player::extension(&mime)) {
                Ok(f) => f,
                Err(e) => return (Task::none(), Some(format!("Couldn't prepare it for playing: {e}"))),
            };
            let Some(gst) = crate::player::gst() else {
                // No GStreamer here: the system's own player.
                if open::that_detached(&file).is_err() {
                    return (Task::none(), Some("No media player found — use Save".into()));
                }
                state.notices.push("Opened in your media player".into());
                return (Task::none(), None);
            };
            let (task, handle) = Task::run(crate::player::play(gst, file.clone(), video), move |e| msg(ChatMsg::Player(id, e))).abortable();
            state.player = Some(crate::components::chat::media_cards::Playing { message_id: id, started: std::time::Instant::now(), duration, frame: None, _stop: handle.abort_on_drop(), file });
            return (task, None);
        }
        ChatMsg::Player(id, event) => {
            let current = state.player.as_ref().is_some_and(|p| p.message_id == id);
            match event {
                crate::player::Event::Frame(f) if current => {
                    if let Some(p) = state.player.as_mut() {
                        p.frame = Some(f);
                    }
                }
                crate::player::Event::Done(r) if current => {
                    state.player = None;
                    if let Err(e) = r {
                        return (Task::none(), Some(e));
                    }
                }
                _ => {}
            }
            Task::none()
        }
        // The mic / the camera: record until stop, sending what GStreamer wrote.
        ChatMsg::RecordStart(kind) => {
            if state.recording.is_some() {
                return (Task::none(), None);
            }
            let Some(room) = state.selected_room() else { return (Task::none(), None) };
            if room.is_member == Some(false) {
                return (Task::none(), Some("Join this room to send here.".into()));
            }
            let Some(gst) = crate::player::gst() else {
                return (Task::none(), Some("Recording needs GStreamer here (install gstreamer1.0-tools and its plugins)".into()));
            };
            let file = crate::recorder::temp_file();
            state.panel = None;
            let (task, handle) = Task::run(crate::recorder::record(gst, kind, file.clone()), move |e| msg(ChatMsg::Recording(e))).abortable();
            state.recording = Some(Recording { kind, started: std::time::Instant::now(), level: 10.0, bars: Vec::new(), frame: None, file, _stop: handle.abort_on_drop() });
            return (task, None);
        }
        ChatMsg::Recording(event) => {
            match event {
                crate::recorder::Event::Level(l) => {
                    if let Some(r) = state.recording.as_mut() {
                        r.level = l;
                        r.bars.push(l);
                        if r.bars.len() > LIVE_BARS {
                            r.bars.remove(0);
                        }
                    }
                }
                crate::recorder::Event::Frame(f) => {
                    if let Some(r) = state.recording.as_mut() {
                        r.frame = Some(f);
                    }
                }
                crate::recorder::Event::Done(result) => {
                    state.recording = None;
                    return match result {
                        Ok(rec) => match send_recording(state, api, *rec) {
                            Ok(task) => (task, None),
                            Err(e) => (Task::none(), Some(e)),
                        },
                        Err(e) => (Task::none(), Some(e)),
                    };
                }
            }
            Task::none()
        }
        // SIGINT: with `-e` GStreamer then finishes the file, and Done brings it back.
        ChatMsg::RecordStop => {
            crate::recorder::stop();
            Task::none()
        }
        ChatMsg::RecordCancel => {
            cancel_recording(state);
            Task::none()
        }
        ChatMsg::TypingTick => {
            let now = std::time::Instant::now();
            state.clock = state.clock_start.elapsed().as_millis() as u64;
            state.typing.retain(|_, who| {
                who.retain(|(_, _, until)| *until > now);
                !who.is_empty()
            });
            Task::none()
        }
        ChatMsg::Send => {
            state.typing_sent = None; // the next keystroke says "typing" again at once
            send(state, api).unwrap_or_else(|e| {
                error = Some(e);
                Task::none()
            })
        }
        ChatMsg::Sent(result) => {
            state.sending = false;
            super::chat_polls::on_sent(state, result.is_ok());
            match result {
                Ok(m) => {
                    let keys = room_keys(state, &m.room_id);
                    let m = rooms::open_message(*m, keys.as_deref());
                    state.draft.clear();
                    state.reply_to = None;
                    chat_media::clear_attachment(state);
                    let key = m.room_id.clone();
                    state.rooms.update_room(&key, |r| {
                        r.last_message = Some(m.content.clone());
                        r.last_timestamp = Some(m.created_at.clone());
                    });
                    state.push_message(m);
                    iced::widget::operation::focus(COMPOSER)
                }
                Err(e) => {
                    error = Some(format!("Couldn't send: {}", e.message));
                    Task::none()
                }
            }
        }
        ChatMsg::Join(key) => {
            if let Some(b) = state.browse.as_mut() {
                b.joining = Some(key.clone());
            }
            let k = key.clone();
            Task::perform(chat::public_rooms::join(api.clone(), key), move |r| msg(ChatMsg::Joined(k.clone(), r.map(|_| ()))))
        }
        // Joined: into the room, with the list reloaded (it's in the sidebar now).
        ChatMsg::Joined(key, Ok(_)) => {
            state.browse = None;
            state.selected = Some(key.clone());
            state.messages.remove(&key);
            Task::batch([load(state, api), load_messages(state, api, &key)])
        }
        ChatMsg::Joined(_, Err(e)) => {
            if let Some(b) = state.browse.as_mut() {
                b.joining = None;
            }
            error = Some(format!("Couldn't join: {}", e.message));
            Task::none()
        }
        ChatMsg::Poll(p) => {
            let (task, e) = super::chat_polls::update(state, p, api);
            error = e;
            task
        }
        ChatMsg::Teaser(t) => {
            use crate::components::chat::group_teaser::TeaserMsg;
            match t {
                TeaserMsg::Note(a) => state.teaser_note.perform(a),
                TeaserMsg::Ask(key) if !state.teaser_busy => {
                    state.teaser_busy = true;
                    let note: String = state.teaser_note.text().trim().chars().take(crate::components::chat::join_invite::NOTE_MAX).collect();
                    return (Task::perform(chat::request_to_join_shown(api.clone(), key, Some(note)), |r| msg(ChatMsg::Teaser(TeaserMsg::Done(r.map(|_| "Request sent — a group admin will let you in").map_err(|e| e.message))))), None);
                }
                TeaserMsg::Withdraw(key) if !state.teaser_busy => {
                    state.teaser_busy = true;
                    return (Task::perform(chat::withdraw_join_request(api.clone(), key), |r| msg(ChatMsg::Teaser(TeaserMsg::Done(r.map(|_| "Request withdrawn").map_err(|e| e.message))))), None);
                }
                TeaserMsg::Ask(_) | TeaserMsg::Withdraw(_) => {}
                TeaserMsg::Done(r) => {
                    state.teaser_busy = false;
                    match r {
                        Ok(done) => {
                            state.teaser_note = Default::default();
                            state.notices.push(done.into());
                            // The card's "Request sent" comes with the list.
                            return (load(state, api), None);
                        }
                        Err(e) => error = Some(e),
                    }
                }
            }
            Task::none()
        }
        ChatMsg::JoinInvite(j) => {
            let (task, e) = join_invite_update(state, j, api);
            error = e;
            task
        }
        ChatMsg::OpenBrowse => {
            state.browse = Some(Browse::default());
            search_catalog(api, String::new())
        }
        ChatMsg::CloseBrowse => {
            state.browse = None;
            Task::none()
        }
        ChatMsg::BrowseQuery(q) => {
            if let Some(b) = state.browse.as_mut() {
                b.query = q;
            }
            Task::none()
        }
        ChatMsg::BrowseSearch => match state.browse.as_mut() {
            Some(b) => {
                b.results = None;
                search_catalog(api, b.query.clone())
            }
            None => Task::none(),
        },
        ChatMsg::BrowseLoaded(result) => {
            match (state.browse.as_mut(), result) {
                (Some(b), Ok(catalog)) => b.results = Some(catalog),
                (Some(b), Err(e)) => {
                    b.results = Some(Catalog::default());
                    error = Some(format!("Couldn't load public rooms: {}", e.message));
                }
                (None, _) => {}
            }
            Task::none()
        }
        ChatMsg::RoomsLoaded(result) => {
            if let Err(e) = &result {
                error = Some(format!("Couldn't load chats: {}", e.message));
            }
            let again = state.rooms.finish_load(result);
            // What the live presence events said stays truer than the list's snapshot.
            apply_presence(state);
            // The open room may have gone (left, deleted).
            if state.selected.as_deref().is_some_and(|k| state.rooms.get(k).is_none()) && state.rooms.loaded {
                state.selected = None;
            }
            open_previews(state);
            let mut tasks = vec![];
            // The open room was locked and its key just arrived (someone shared it): read it again.
            if let Some(open) = state.selected.clone().filter(|k| !state.rooms.get(k).is_some_and(display::is_teaser)) {
                // A group card that just became a group (the request was approved): its feed.
                if !state.messages.contains_key(&open) {
                    tasks.push(load_messages(state, api, &open));
                }
                let was_locked = state.locked.contains_key(&open);
                if room_keys(state, &open).is_some() && was_locked {
                    state.messages.remove(&open);
                    tasks.push(load_messages(state, api, &open));
                }
                tasks.push(look_up_sharer(state, api, &open));
                // Membership may have changed: anyone new gets the key.
                tasks.push(room_admin::heal(state, api, &open));
            }
            if state.manage.as_ref().is_some_and(|m| state.rooms.get(&m.room_key).is_none()) {
                state.manage = None;
            }
            if again {
                tasks.push(Task::perform(chat::get_rooms(api.clone()), |r| msg(ChatMsg::RoomsLoaded(r))));
            }
            Task::batch(tasks)
        }
        ChatMsg::UnreadsLoaded(Ok(u)) => {
            state.unreads = u.unreads;
            Task::none()
        }
        ChatMsg::UnreadsLoaded(Err(_)) => Task::none(),
        ChatMsg::MessagesLoaded(key, result) => {
            state.loading.remove(&key);
            match result {
                Ok(mut list) => {
                    list.sort_by_key(|m| m.id);
                    list.retain(|m| m.thread_root_id.is_none());
                    let keys = room_keys(state, &key);
                    let list = list.into_iter().map(|m| rooms::open_message(m, keys.as_deref())).collect();
                    state.messages.insert(key.clone(), list);
                    let lookup = look_up_sharer(state, api, &key);
                    let read = if state.selected.as_deref() == Some(key.as_str()) { mark_read(state, api, &key) } else { Task::none() };
                    Task::batch([read, lookup])
                }
                Err(e) => {
                    error = Some(format!("Couldn't load messages: {}", e.message));
                    Task::none()
                }
            }
        }
        ChatMsg::Done => Task::none(),
        ChatMsg::SharerKey(id, result) => {
            state.looking_up.remove(&id);
            match result.ok().and_then(|u| u.e2ee_public_key) {
                Some(public) => {
                    state.keys.remember_public_key(id, public);
                    // Rooms locked for want of this key: open them again.
                    let waiting: Vec<String> = state.locked.iter().filter(|(_, p)| **p == KeyProblem::UnknownSharer(id)).map(|(k, _)| k.clone()).collect();
                    let mut tasks = vec![];
                    for key in waiting {
                        if room_keys(state, &key).is_some() && state.messages.remove(&key).is_some() {
                            tasks.push(load_messages(state, api, &key));
                        }
                    }
                    open_previews(state);
                    Task::batch(tasks)
                }
                None => {
                    state.locked.iter_mut().filter(|(_, p)| **p == KeyProblem::UnknownSharer(id)).for_each(|(_, p)| *p = KeyProblem::NoEnvelope);
                    Task::none()
                }
            }
        }
        ChatMsg::OpenManage(tab) => room_admin::open(state, tab, api),
        ChatMsg::Menu(id, kind) => {
            chat_actions::open_menu(state, id, kind);
            Task::none()
        }
        ChatMsg::CloseMenu => {
            state.menu = None;
            Task::none()
        }
        ChatMsg::React(id, emoji) => {
            state.emoji_ui.used(&emoji);
            chat_actions::react(state, api, id, emoji)
        }
        ChatMsg::Reply(id) => chat_actions::reply(state, id),
        ChatMsg::CancelReply => {
            state.reply_to = None;
            Task::none()
        }
        ChatMsg::Edit(id) => chat_actions::edit(state, id),
        ChatMsg::CancelEdit => {
            chat_actions::cancel_edit(state);
            Task::none()
        }
        ChatMsg::Delete(id) => chat_actions::delete(state, api, id),
        ChatMsg::TogglePin(id) => chat_actions::toggle_pin(state, api, id),
        ChatMsg::CopyText(id) => chat_actions::copy(state, id, false),
        ChatMsg::CopyId(id) => chat_actions::copy(state, id, true),
        ChatMsg::OpenThread(id) => {
            state.members_open = false;
            chat_actions::open_thread(state, api, id)
        }
        ChatMsg::ToggleMembers => {
            state.members_open = !state.members_open;
            state.member_menu = None;
            if state.members_open {
                state.thread = None;
                load_members(state, api, 0)
            } else {
                Task::none()
            }
        }
        ChatMsg::MemberSearch(q) => {
            state.member_search = q;
            load_members(state, api, 0)
        }
        ChatMsg::MoreMembers => {
            let offset = state.members_paged.as_ref().map_or(0, |p| p.members.len() as i64);
            load_members(state, api, offset)
        }
        ChatMsg::MembersLoaded(key, query, result) => {
            if let Some(p) = state.members_paged.as_mut().filter(|p| p.room_key == key && p.query == query) {
                p.loading = false;
                match result {
                    Ok(page) => {
                        p.total = page.total;
                        if page.offset == 0 {
                            p.members = page.members;
                        } else {
                            let have: HashSet<i64> = p.members.iter().map(|u| u.id).collect();
                            p.members.extend(page.members.into_iter().filter(|u| !have.contains(&u.id)));
                        }
                    }
                    Err(e) => error = Some(format!("Couldn't load members: {}", e.message)),
                }
                apply_presence(state);
            }
            Task::none()
        }
        ChatMsg::MemberMenu(id) => {
            state.member_menu = if state.member_menu == Some(id) { None } else { Some(id) };
            Task::none()
        }
        ChatMsg::CloseMemberMenu => {
            state.member_menu = None;
            Task::none()
        }
        ChatMsg::Mention(username) => {
            state.member_menu = None;
            if !state.draft.is_empty() && !state.draft.ends_with(' ') {
                state.draft.push(' ');
            }
            state.draft.push_str(&format!("@{username} "));
            iced::widget::operation::focus(COMPOSER)
        }
        ChatMsg::MessageUser(id, username) => {
            state.member_menu = None;
            let dm = state.rooms.rooms.iter().find(|r| display::is_dm(r) && r.allowed_user_ids.contains(&id) && r.allowed_user_ids.contains(&state.me_id)).map(|r| r.room_key.clone());
            match dm {
                Some(key) => Task::done(msg(ChatMsg::OpenRoom(key))),
                None => {
                    let mut form = new_chat_modal::State::new(state.can_create_public);
                    form.dm_username = username;
                    state.new_chat = Some(form);
                    Task::none()
                }
            }
        }
        ChatMsg::CopyValue(value, note) => {
            state.member_menu = None;
            state.notices.push(note.into());
            iced::clipboard::write(value)
        }
        ChatMsg::MemberPermissions(id) => {
            state.member_menu = None;
            let task = room_admin::open(state, Tab::Members, api);
            if let Some(m) = state.manage.as_mut() {
                m.expanded = Some(id);
            }
            task
        }
        ChatMsg::KickMember(id, name) => {
            state.member_menu = None;
            let task = room_admin::open(state, Tab::Members, api);
            if let Some(m) = state.manage.as_mut() {
                m.confirm = Some(manage_room::Confirm::Kick(id, name));
            }
            task
        }
        ChatMsg::OpenProfile(user) => {
            state.member_menu = None;
            state.menu = None;
            let user = *user;
            let (id, username, me) = (user.id, user.username.clone(), user.id == state.me_id);
            state.profile = Some(user_profile::State::new(user, state.selected.clone()));
            let mut tasks = vec![Task::perform(chat::get_user_profile(api.clone(), id.to_string()), move |r| msg(ChatMsg::ProfileLoaded(id, r.map(Box::new))))];
            if !me {
                tasks.push(Task::perform(chat::get_user_relationship(api.clone(), crate::api::client::enc(&username)), move |r| msg(ChatMsg::ProfileRelationship(id, r))));
            }
            Task::batch(tasks)
        }
        ChatMsg::ProfileLoaded(id, result) => {
            if let Some(p) = state.profile.as_mut().filter(|p| p.user.id == id) {
                p.loaded = true;
                if let Ok(full) = result {
                    let mut full = *full;
                    // Presence comes with the room's member list; the profile may not carry it.
                    if full.is_online.is_none() {
                        full.is_online = p.user.is_online;
                    }
                    if full.last_seen_at.is_none() {
                        full.last_seen_at = p.user.last_seen_at.clone();
                    }
                    if full.e2ee_public_key.is_none() {
                        full.e2ee_public_key = p.user.e2ee_public_key.clone();
                    }
                    p.user = full;
                }
            }
            apply_presence(state);
            Task::none()
        }
        ChatMsg::ProfileRelationship(id, result) => {
            if let Some(p) = state.profile.as_mut().filter(|p| p.user.id == id) {
                p.relationship = Some(result.unwrap_or_default());
            }
            Task::none()
        }
        ChatMsg::Profile(pm) => {
            let Some(p) = state.profile.as_mut() else { return (Task::none(), None) };
            let user = p.user.clone();
            let pay = format!("{}/pay/{}", crate::api::client::normalize_base(api.base()), user.username);
            match pm {
                ProfileMsg::Tab(t) => {
                    p.tab = t;
                    Task::none()
                }
                ProfileMsg::Close => {
                    state.profile = None;
                    Task::none()
                }
                ProfileMsg::Quick(v) => {
                    p.quick = v;
                    Task::none()
                }
                ProfileMsg::SendQuick => {
                    let text = p.quick.trim().to_string();
                    state.profile = None;
                    let dm = state.rooms.rooms.iter().find(|r| display::is_dm(r) && r.allowed_user_ids.contains(&user.id) && r.allowed_user_ids.contains(&state.me_id)).map(|r| r.room_key.clone());
                    match dm {
                        Some(key) => {
                            let (open, e) = update(state, ChatMsg::OpenRoom(key), api);
                            error = e;
                            if text.is_empty() {
                                open
                            } else {
                                state.draft = text;
                                let sent = send(state, api).unwrap_or_else(|e| {
                                    error = Some(e);
                                    Task::none()
                                });
                                Task::batch([open, sent])
                            }
                        }
                        None => {
                            // No DM yet: make one (its key), the text waits in the box.
                            let mut form = new_chat_modal::State::new(state.can_create_public);
                            form.dm_username = user.username.clone();
                            state.new_chat = Some(form);
                            if !text.is_empty() {
                                state.draft = text;
                            }
                            Task::none()
                        }
                    }
                }
                ProfileMsg::CopyId => {
                    state.notices.push("User ID copied".into());
                    iced::clipboard::write(user.id.to_string())
                }
                ProfileMsg::CopyPayLink => {
                    state.notices.push("Payment link copied".into());
                    iced::clipboard::write(pay)
                }
                ProfileMsg::OpenPayLink => {
                    if open::that_detached(&pay).is_err() {
                        state.notices.push(format!("Open {pay} in your browser"));
                    }
                    Task::none()
                }
                ProfileMsg::CopyAddress(a) => {
                    state.notices.push("Address copied".into());
                    iced::clipboard::write(a)
                }
                ProfileMsg::OpenRoom(key) => {
                    state.profile = None;
                    let (t, e) = update(state, ChatMsg::OpenRoom(key), api);
                    error = e;
                    t
                }
                ProfileMsg::Cheque => Task::done(Message::NotYet("Writing a cheque from someone's card")),
                ProfileMsg::Invoice => Task::done(Message::NotYet("Issuing an invoice from someone's card")),
                ProfileMsg::EditProfile => {
                    state.profile = None;
                    Task::done(Message::Navigate(crate::app::Nav::Settings))
                }
                ProfileMsg::Permissions => {
                    state.profile = None;
                    let (t, e) = update(state, ChatMsg::MemberPermissions(user.id), api);
                    error = e;
                    t
                }
                ProfileMsg::Remove => {
                    state.profile = None;
                    let (t, e) = update(state, ChatMsg::KickMember(user.id, user.username.clone()), api);
                    error = e;
                    t
                }
            }
        }
        ChatMsg::AttachPath(path) => chat_media::attach(state, path),
        ChatMsg::AttachFinance(kind, item) => {
            chat_media::clear_attachment(state);
            state.pending_finance = Some((kind, *item));
            iced::widget::operation::focus(COMPOSER)
        }
        ChatMsg::ClearAttachment => {
            chat_media::clear_attachment(state);
            Task::none()
        }
        ChatMsg::SaveAttachment(id) => chat_media::save(state, id),
        ChatMsg::OpenImage(id) => {
            state.lightbox = Some(id);
            Task::none()
        }
        ChatMsg::CloseImage => {
            if let Some(id) = state.lightbox.take() {
                state.media.forget(&format!("big:{id}"));
            }
            Task::none()
        }
        ChatMsg::OpenEmoji => {
            chat_media::open_emoji(state);
            iced::widget::operation::focus(crate::components::chat::emoji_picker::EMOJI_SEARCH)
        }
        ChatMsg::EmojiCategory(i) => {
            state.emoji_ui.category = i;
            state.emoji_ui.query.clear();
            Task::none()
        }
        ChatMsg::EmojiQuery(q) => {
            state.emoji_ui.query = q;
            Task::none()
        }
        ChatMsg::Fund(fm) => {
            use crate::components::chat::fund_cards::FundMsg;
            match fm {
                FundMsg::Campaign(id, r) => {
                    state.fund.campaigns.insert(id, Some(r.map(|b| *b)));
                    Task::none()
                }
                FundMsg::Drop(id, r) => {
                    let card = state.fund.drops.entry(id.clone()).or_default();
                    match r {
                        Ok(b) => {
                            let (info, slots) = *b;
                            card.info = Some(Ok(info));
                            card.slots = slots;
                            card.chain = None;
                            card.unproven.clear();
                            chat_media::read_drop_on_chain(state, &id)
                        }
                        Err(e) => {
                            card.loading = false;
                            // A failed refresh keeps what the card showed.
                            if !matches!(card.info, Some(Ok(_))) {
                                card.info = Some(Err(e));
                            }
                            Task::none()
                        }
                    }
                }
                FundMsg::OnChain(id, r) => {
                    let card = state.fund.drops.entry(id).or_default();
                    card.loading = false;
                    match r {
                        #[cfg(feature = "drops")]
                        Ok(c) => {
                            crate::components::chat::drop_card::apply(card, *c);
                            state.live_times |= crate::components::chat::drop_card::ticking(card, chrono::Utc::now().timestamp().max(0) as u64);
                        }
                        #[cfg(not(feature = "drops"))]
                        Ok(_) => {}
                        Err(e) => card.chain = Some(Err(e)),
                    }
                    Task::none()
                }
                FundMsg::CheckOpen(id) => {
                    let c = &mut state.fund.drops.entry(id).or_default().check;
                    c.open = !c.open;
                    Task::none()
                }
                FundMsg::CheckAddress(id, v) => {
                    let c = &mut state.fund.drops.entry(id).or_default().check;
                    c.address = v.trim().to_string();
                    c.result = None;
                    Task::none()
                }
                FundMsg::Check(id) => {
                    let c = &mut state.fund.drops.entry(id.clone()).or_default().check;
                    if c.busy || !crate::components::chat::fund_cards::is_address(&c.address) {
                        Task::none()
                    } else {
                        c.busy = true;
                        let addr = c.address.clone();
                        let lookup = crate::api::drops::lookup(api.clone(), id.clone(), vec![addr.clone()]);
                        Task::perform(lookup, move |r| {
                            let share = r.map(|slots| slots.into_iter().next().map(Box::new)).map_err(|e| e.message);
                            Message::Chat(ChatMsg::Fund(FundMsg::Checked(id.clone(), addr.clone(), share)))
                        })
                    }
                }
                FundMsg::Checked(id, addr, r) => {
                    if let Some(card) = state.fund.drops.get_mut(&id) {
                        card.check.busy = false;
                        if card.check.address.eq_ignore_ascii_case(&addr) {
                            card.check.result = Some((addr, r.map(|s| s.map(|b| *b))));
                        }
                    }
                    Task::none()
                }
                FundMsg::Reload(id) => {
                    if state.fund.campaigns.contains_key(&id) {
                        chat_media::load_campaign_card(state, api, &id)
                    } else {
                        chat_media::load_drop_card(state, api, &id)
                    }
                }
            }
        }
        ChatMsg::Invoice(im) => {
            use crate::components::chat::invoice_card::{InvoiceMsg, Tab};
            match im {
                InvoiceMsg::Tab(id, t) => {
                    state.invoices.entry(id.clone()).or_default().tab = t;
                    if t == Tab::History {
                        return (chat_media::load_invoice(state, api, &id), None);
                    }
                    Task::none()
                }
                InvoiceMsg::Loaded(id, r) => {
                    if let (Some(v), Ok(live)) = (state.invoices.get_mut(&id), r) {
                        v.live = Some(live);
                    }
                    Task::none()
                }
                InvoiceMsg::Payments(id, r) => {
                    if let Some(v) = state.invoices.get_mut(&id) {
                        v.loading = false;
                        v.payments = Some(r.unwrap_or_default());
                    }
                    Task::none()
                }
                InvoiceMsg::Refresh(id) => chat_media::load_invoice(state, api, &id),
            }
        }
        ChatMsg::Cheque(cm) => {
            use crate::components::chat::cheque_card::ChequeMsg;
            match cm {
                ChequeMsg::Tab(id, t) => {
                    let v = state.cheques.entry(id.clone()).or_default();
                    v.tab = t;
                    if t == crate::components::chat::cheque_card::Tab::History {
                        return (chat_media::load_cheque_claims(state, api, &id), None);
                    }
                    Task::none()
                }
                ChequeMsg::Loaded(id, r) => {
                    if let (Some(v), Ok(live)) = (state.cheques.get_mut(&id), r) {
                        v.live = Some(live);
                    }
                    Task::none()
                }
                ChequeMsg::Claims(id, r) => {
                    if let Some(v) = state.cheques.get_mut(&id) {
                        v.loading_claims = false;
                        v.claims = Some(r.unwrap_or_default());
                    }
                    Task::none()
                }
                ChequeMsg::RefreshClaims(id) => chat_media::load_cheque_claims(state, api, &id),
                ChequeMsg::Copy(value, what) => {
                    state.notices.push(format!("{what} copied!"));
                    iced::clipboard::write(value)
                }
                ChequeMsg::OpenTx(url) => {
                    if open::that_detached(&url).is_err() {
                        state.notices.push(format!("Open {url} in your browser"));
                    }
                    Task::none()
                }
                ChequeMsg::Claim(c, recipient) => Task::done(Message::ClaimCheque(c, Some(recipient))),
            }
        }
        ChatMsg::EmojiSkin => {
            state.emoji_ui.skin = (state.emoji_ui.skin + 1) % crate::components::chat::emoji_picker::SKINS.len();
            Task::none()
        }
        ChatMsg::OpenGifs => chat_media::open_gifs(state, api),
        ChatMsg::ClosePanel => {
            state.panel = None;
            Task::none()
        }
        ChatMsg::InsertEmoji(e) => {
            state.emoji_ui.used(&e);
            state.draft.push_str(&e);
            iced::widget::operation::focus(COMPOSER)
        }
        ChatMsg::GifTab(t) => {
            if let Some(Panel::Gifs(p)) = state.panel.as_mut() {
                p.tab = t;
            }
            chat_media::load_gifs(state, api, 0)
        }
        ChatMsg::GifQuery(q) => {
            let cleared = q.trim().is_empty();
            if let Some(Panel::Gifs(p)) = state.panel.as_mut() {
                p.query = q;
            }
            // Emptied: back to trending; otherwise Enter searches.
            if cleared { chat_media::load_gifs(state, api, 0) } else { Task::none() }
        }
        ChatMsg::GifSearch => chat_media::load_gifs(state, api, 0),
        ChatMsg::GifCategory(q) => {
            if let Some(Panel::Gifs(p)) = state.panel.as_mut() {
                p.query = q;
            }
            chat_media::load_gifs(state, api, 0)
        }
        ChatMsg::GifScrolled => Task::none(),
        ChatMsg::MoreGifs => {
            let offset = match &state.panel {
                Some(Panel::Gifs(p)) => p.items.as_ref().map_or(0, |i| i.len() as i64),
                _ => 0,
            };
            chat_media::load_gifs(state, api, offset)
        }
        ChatMsg::SendGif(g) => chat_media::send_gif(state, api, &g).unwrap_or_else(|e| {
            error = Some(e);
            Task::none()
        }),
        ChatMsg::SendSticker(s) => chat_media::send_sticker(state, api, &s).unwrap_or_else(|e| {
            error = Some(e);
            Task::none()
        }),
        ChatMsg::Media(r) => {
            let (t, e) = chat_media::on_result(state, *r, api);
            error = e;
            t
        }
        ChatMsg::OpenPlus => {
            state.panel = if matches!(state.panel, Some(Panel::Plus)) { None } else { Some(Panel::Plus) };
            Task::none()
        }
        ChatMsg::InsertTime(tok) => {
            state.panel = None;
            if !state.draft.is_empty() && !state.draft.ends_with(' ') {
                state.draft.push(' ');
            }
            state.draft.push_str(&tok);
            state.draft.push(' ');
            iced::widget::operation::focus(COMPOSER)
        }
        ChatMsg::Tick => {
            state.clock = state.clock_start.elapsed().as_millis() as u64;
            // A recording that reached its length finishes by itself (the web's limit).
            if state.recording.as_ref().is_some_and(|r| r.elapsed() >= r.limit()) {
                crate::recorder::stop();
            }
            Task::none()
        }
        ChatMsg::OpenPinned => {
            state.pinned_open = true;
            Task::none()
        }
        ChatMsg::Search(m) => super::chat_search::update(state, m, api),
        ChatMsg::ClosePinned => {
            state.pinned_open = false;
            Task::none()
        }
        ChatMsg::JumpTo(id) => {
            state.pinned_open = false;
            let feed = state.selected.as_ref().and_then(|k| state.messages.get(k));
            match feed.and_then(|f| f.iter().find(|m| m.id == id)) {
                Some(_) => {
                    // Marked (it carries the id the scroll looks for), then scrolled to where it is.
                    state.flash = Some(id);
                    Task::batch([
                        message_list::scroll_to_flashed(),
                        Task::perform(tokio::time::sleep(std::time::Duration::from_millis(1600)), move |_| msg(ChatMsg::Unflash(id))),
                    ])
                }
                None => {
                    state.notices.push("That message isn't loaded here".into());
                    Task::none()
                }
            }
        }
        ChatMsg::Unflash(id) => {
            if state.flash == Some(id) {
                state.flash = None;
            }
            Task::none()
        }
        ChatMsg::CloseThread => {
            state.thread = None;
            Task::none()
        }
        ChatMsg::ThreadDraft(v) => {
            if let Some(t) = state.thread.as_mut() {
                t.draft = v;
            }
            Task::none()
        }
        ChatMsg::ThreadSend => chat_actions::send_in_thread(state, api).unwrap_or_else(|e| {
            error = Some(e);
            Task::none()
        }),
        ChatMsg::Action(result) => {
            let (task, e) = chat_actions::on_result(state, *result, api);
            error = e;
            task
        }
        ChatMsg::Escape => {
            if state.recording.is_some() {
                cancel_recording(state);
            } else if state.join.is_some() {
                state.join = None;
            } else if state.lightbox.is_some() {
                state.lightbox = None;
            } else if state.panel.is_some() {
                state.panel = None;
            } else if state.menu.is_some() || state.member_menu.is_some() {
                state.menu = None;
                state.member_menu = None;
            } else if state.profile.is_some() {
                state.profile = None;
            } else if state.pinned_open {
                state.pinned_open = false;
            } else if state.search.is_some() {
                state.search = None;
            } else if state.editing.is_some() {
                chat_actions::cancel_edit(state);
            } else if state.reply_to.is_some() {
                state.reply_to = None;
            }
            Task::none()
        }
        ChatMsg::Manage(m) => {
            let (task, e) = room_admin::update(state, m, api);
            error = e;
            task
        }
        ChatMsg::Admin(result) => {
            let (task, e) = room_admin::on_result(state, *result, api);
            error = e;
            task
        }
        ChatMsg::OpenNewChat(kind) => {
            let mut form = new_chat_modal::State::new(state.can_create_public);
            form.kind = kind;
            state.new_chat = Some(form);
            Task::none()
        }
        ChatMsg::NewChat(NewChatMsg::Close) => {
            state.new_chat = None;
            Task::none()
        }
        ChatMsg::NewChat(NewChatMsg::Submit) => submit_new_chat(state, api),
        ChatMsg::NewChat(m) => {
            if let Some(form) = state.new_chat.as_mut() {
                form.update(m);
            }
            Task::none()
        }
        ChatMsg::NewChatProfiles(profiles) => create_with_profiles(state, api, profiles),
        ChatMsg::NewChatCreated(Ok(room)) => {
            state.new_chat = None;
            let key = room.room_key.clone();
            state.rooms.upsert(*room);
            state.selected = Some(key.clone());
            state.messages.remove(&key);
            room_keys(state, &key);
            Task::batch([load(state, api), load_messages(state, api, &key), iced::widget::operation::focus(COMPOSER)])
        }
        ChatMsg::NewChatCreated(Err(e)) => {
            if let Some(form) = state.new_chat.as_mut() {
                form.busy = false;
                form.error = Some(e);
            }
            Task::none()
        }
    };
    (task, error)
}

/// A socket event (ChatSync.tsx + the chat page's own listeners).
pub fn on_socket(state: &mut State, event: &Value, api: &Api) -> Task<Message> {
    let name = event.get("event").and_then(Value::as_str).unwrap_or_default();
    let room_id = event.get("room_id").and_then(Value::as_str).unwrap_or_default().to_string();
    let message_id = event.get("message_id").and_then(Value::as_i64);
    let parse = |v: &Value| v.get("message").and_then(|m| serde_json::from_value::<ChatMessageItem>(m.clone()).ok());
    match name {
        "chat_message" => {
            let Some(m) = parse(event) else { return Task::none() };
            super::chat_search::observe(state, &m);
            // Their message is here: they've stopped typing.
            if let (Some(who), Some(uid)) = (state.typing.get_mut(&m.room_id), m.sender_id) {
                who.retain(|(u, _, _)| *u != uid);
            }
            if m.thread_root_id.is_some() {
                // Not in the main feed: into the open thread, and the root's "N replies".
                let summary = event.get("thread_summary").and_then(|s| serde_json::from_value(s.clone()).ok());
                chat_actions::on_thread_reply(state, m, summary);
                return Task::none();
            }
            if state.rooms.get(&m.room_id).is_none() {
                return load(state, api); // a room we don't have yet
            }
            let keys = room_keys(state, &m.room_id);
            let m = rooms::open_message(m, keys.as_deref());
            let key = m.room_id.clone();
            let mine = m.sender_id == Some(state.me_id);
            state.rooms.update_room(&key, |r| {
                r.last_message = Some(preview_of(&m));
                r.last_timestamp = Some(m.created_at.clone());
            });
            state.push_message(m);
            if state.selected.as_deref() == Some(key.as_str()) {
                mark_read(state, api, &key)
            } else {
                if !mine {
                    state.unreads.entry(key).or_default().unread_count += 1;
                }
                Task::none()
            }
        }
        "message_edited" | "message_updated" => {
            if let Some(m) = parse(event) {
                super::chat_search::observe(state, &m);
                let keys = room_keys(state, &m.room_id);
                let m = rooms::open_message(m, keys.as_deref());
                if let Some(t) = state.thread.as_mut() {
                    if let Some(x) = t.find_mut(m.id) {
                        *x = m.clone();
                    }
                }
                state.push_message(m);
            }
            Task::none()
        }
        "poll_updated" => match message_id {
            Some(id) => super::chat_polls::on_update(state, api, id),
            None => Task::none(),
        },
        "user_typing" => {
            let Some(uid) = event.get("user_id").and_then(Value::as_i64).filter(|u| *u != state.me_id) else { return Task::none() };
            let name = ["display_name", "username"].iter().find_map(|k| event.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty())).unwrap_or("Someone").to_string();
            let who = state.typing.entry(room_id).or_default();
            who.retain(|(u, _, _)| *u != uid);
            who.push((uid, name, std::time::Instant::now() + TYPING_SHOWN));
            Task::none()
        }
        // Someone's name or picture changed (UpdateProfile): every copy of them here, no request.
        "user_profile_updated" => {
            let Some(u) = event.get("user").filter(|u| u.is_object()) else { return Task::none() };
            let Some(id) = u.get("id").and_then(Value::as_i64) else { return Task::none() };
            let text = |k: &str| u.get(k).and_then(Value::as_str).map(str::to_string);
            let (username, display, avatar) = (text("username"), text("display_name"), text("avatar"));
            let fix = |m: &mut UserInfo| {
                if m.id != id {
                    return;
                }
                if let Some(n) = &username {
                    m.username = n.clone();
                }
                m.display_name = display.clone().unwrap_or_default();
                m.avatar = avatar.clone();
            };
            for r in state.rooms.rooms.iter_mut() {
                r.allowed_users.iter_mut().for_each(fix);
            }
            if let Some(p) = state.members_paged.as_mut() {
                p.members.iter_mut().for_each(fix);
            }
            if let Some(p) = state.profile.as_mut() {
                fix(&mut p.user);
            }
            Task::none()
        }
        "user_presence" => {
            if let Some(uid) = event.get("user_id").and_then(Value::as_i64) {
                let online = event.get("is_online").and_then(Value::as_bool).unwrap_or(false);
                let seen = event.get("last_seen_at").and_then(Value::as_str).map(str::to_string);
                state.presence.insert(uid, (online, seen));
                apply_presence(state);
            }
            Task::none()
        }
        "messages_expired" => {
            // Disappearing messages whose time is up (chat_timers): off the screen too.
            let ids: HashSet<i64> = event.get("message_ids").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_i64).collect();
            if let Some(feed) = state.messages.get_mut(&room_id) {
                feed.retain(|m| !ids.contains(&m.id));
            }
            if state.thread.as_ref().is_some_and(|t| ids.contains(&t.root_id)) {
                state.thread = None;
            } else if let Some(t) = state.thread.as_mut() {
                t.replies.retain(|m| !ids.contains(&m.id));
            }
            state.polls.retain(|id, _| !ids.contains(id));
            super::chat_search::forget(state, &ids);
            Task::none()
        }
        "message_deleted" => {
            if let (Some(feed), Some(id)) = (state.messages.get_mut(&room_id), message_id) {
                feed.retain(|m| m.id != id);
            }
            if let Some(id) = message_id {
                super::chat_search::forget(state, &HashSet::from([id]));
            }
            if let (Some(t), Some(id)) = (state.thread.as_mut(), message_id) {
                t.replies.retain(|m| m.id != id);
            }
            Task::none()
        }
        "chat_reaction" => {
            if let Some(id) = message_id {
                let reactions: Option<HashMap<String, Vec<i64>>> = event.get("reactions").and_then(|r| serde_json::from_value(r.clone()).ok());
                chat_actions::change(state, id, |m| m.reactions = reactions.clone());
            }
            Task::none()
        }
        "chat_pin" => {
            if let Some(id) = message_id {
                let pinned = event.get("is_pinned").and_then(Value::as_bool);
                chat_actions::change(state, id, |m| m.is_pinned = pinned);
                super::chat_search::set_pinned(state, id, pinned == Some(true));
            }
            Task::none()
        }
        // Read on another device of mine.
        "room_read" if event.get("user_id").and_then(Value::as_i64) == Some(state.me_id) => {
            if let Some(u) = state.unreads.get_mut(&room_id) {
                u.unread_count = 0;
                u.mention_count = 0;
            }
            Task::none()
        }
        "room_join_request" => {
            let key = event.get("room_key").and_then(Value::as_str).unwrap_or(&room_id).to_string();
            Task::batch([load(state, api), room_admin::on_join_request(state, api, &key)])
        }
        "room_created" | "room_updated" | "room_left" | "room_kicked" | "room_deleted" => load(state, api),
        _ => Task::none(),
    }
}

/// After the socket came back: what happened in the gap is re-fetched.
pub fn on_reconnect(state: &mut State, api: &Api) -> Task<Message> {
    let open = state.selected.clone();
    state.messages.retain(|k, _| Some(k) == open.as_ref());
    let mut tasks = vec![load(state, api)];
    if let Some(key) = open {
        tasks.push(load_messages(state, api, &key));
    }
    Task::batch(tasks)
}

pub fn view(state: &State) -> El<'_> {
    let conversation: El = match state.selected_room() {
        Some(room) if display::is_teaser(room) => crate::components::chat::group_teaser::view(room, state.me_id, &state.teaser_note, state.teaser_busy),
        Some(room) => {
            let rights = Rights {
                me: state.me_id,
                pin: crate::store::room_permissions::effective(room, state.me_id, state.is_admin).contains(&crate::store::room_permissions::Perm::PinMessages),
                moderate: room.created_by == Some(state.me_id),
                threads: true,
                write: room.is_member != Some(false),
            };
            let feed = Feed {
                messages: state.messages.get(&room.room_key).map(Vec::as_slice),
                loading: state.loading.contains(&room.room_key),
                rights,
                menu: state.menu,
                members: &room.allowed_users,
                flash: state.flash,
                media: &state.media,
                clock: state.clock,
                emoji: &state.emoji_ui,
                cheques: &state.cheques,
                explorers: &state.explorers,
                wallets: &state.my_wallets,
                invoices: &state.invoices,
                me_name: &state.me_name,
                fund: &state.fund,
                conceal: state.blur_messages,
                polls: Some(&state.polls),
                private: display::is_encrypted_room(room),
                playing: state.player.as_ref(),
            };
            let mode = match (&state.reply_to, state.editing) {
                (_, Some(_)) => composer::Mode::Edit,
                (Some(m), None) => composer::Mode::Reply(m),
                _ => composer::Mode::Write,
            };
            let main = column![
                chat_header::view(room, state.me_id, state.rooms.get(&room.room_key).and_then(|r| r.join_requests_count).unwrap_or(0), state.members_open, state.search.as_ref().is_some_and(|s| s.room_key == room.room_key)),
                hline(),
                feed_with_panel(state, message_list::view(feed)),
                composer::view(room, &state.draft, state.sending, state.locked.get(&room.room_key), mode, state.pending_file.as_ref(), state.media.frames("pending").map(|f| f.at(0)), state.pending_finance.as_ref(), state.recording.as_ref(), crate::player::gst().is_some()),
                typing_bar(state.typing.get(&room.room_key).map(Vec::as_slice).unwrap_or(&[]), state.clock),
            ]
            .width(Fill)
            .height(Fill);
            let search = state.search.as_ref().filter(|s| s.room_key == room.room_key);
            match (state.thread.as_ref().filter(|t| t.room_key == room.room_key), search) {
                (Some(t), _) => {
                    let locked = state.locked.get(&room.room_key).filter(|_| display::is_encrypted_room(room)).map(KeyProblem::explain);
                    row![main, thread_panel::view(t, rights, state.menu, &room.allowed_users, display::is_encrypted_room(room), locked, &state.media, state.clock, &state.emoji_ui, (&state.cheques, &state.explorers, &state.my_wallets, &state.invoices, &state.me_name, &state.fund))].into()
                }
                (None, Some(s)) => row![main, search_panel::view(s, state.search_index.get(&room.room_key), display::is_encrypted_room(room), &state.me_name)].into(),
                (None, None) if state.members_open && !display::is_dm(room) => {
                    let side = members_sidebar::view(members_sidebar::View {
                        room,
                        me: state.me_id,
                        is_admin: state.is_admin,
                        search: &state.member_search,
                        paged: state.members_paged.as_ref().filter(|p| p.room_key == room.room_key),
                        pending: state.pending_admissions.get(&room.room_key).map(Vec::as_slice).unwrap_or(&[]),
                        menu: state.member_menu,
                    });
                    row![main, side].into()
                }
                // A row like the ones above, so the feed keeps its place when a side panel opens.
                (None, None) => row![main].into(),
            }
        }
        None => empty_state(state),
    };
    let page: El = row![chat_sidebar::view(state), conversation].into();
    let mut layers = vec![page];
    if let Some(b) = &state.browse {
        layers.push(browse_rooms::view(b));
    }
    if let Some(form) = &state.new_chat {
        layers.push(new_chat_modal::view(form));
    }
    if let Some(f) = &state.poll_form {
        layers.push(crate::components::chat::poll_card::poll_modal(f));
    }
    if let Some(f) = &state.draw_form {
        let members = state.selected_room().map(|r| r.member_count.unwrap_or(r.allowed_users.len() as i64)).unwrap_or(0);
        layers.push(crate::components::chat::poll_card::draw_modal(f, members));
    }
    if let Some(j) = &state.join {
        layers.push(crate::components::chat::join_invite::view(j));
    }
    if let Some(room) = state.selected_room().filter(|_| state.pinned_open) {
        let mut pinned: Vec<&ChatMessageItem> = state.messages.get(&room.room_key).map(|f| f.iter().filter(|m| m.is_pinned == Some(true)).collect()).unwrap_or_default();
        pinned.reverse();
        let can_unpin = crate::store::room_permissions::effective(room, state.me_id, state.is_admin).contains(&crate::store::room_permissions::Perm::PinMessages);
        layers.push(pinned_modal::view(pinned, can_unpin));
    }
    if let Some(id) = state.lightbox {
        layers.push(lightbox(state, id));
    }
    if let Some(p) = &state.profile {
        let room = p.room_key.as_deref().and_then(|k| state.rooms.get(k));
        let pending = room.and_then(|r| state.pending_admissions.get(&r.room_key)).is_some_and(|ids| ids.contains(&p.user.id));
        let trust = p.user.e2ee_public_key.as_deref().and_then(|k| state.keys.trust_of(p.user.id, k));
        layers.push(user_profile::view(p, user_profile::Context { me: state.me_id, is_admin: state.is_admin, room, pending, trust }));
    }
    if let Some(form) = &state.manage {
        if let Some(room) = state.rooms.get(&form.room_key) {
            let pending = state.pending_admissions.get(&room.room_key).map(Vec::as_slice).unwrap_or(&[]);
            layers.push(manage_room::view(form, room, room_admin::access(state, room), pending));
        }
    }
    // The camera's round window: over everything while a video note is recorded (Esc closes it).
    if let Some(r) = state.recording.as_ref().filter(|r| r.note()) {
        layers.push(crate::components::chat::recorder_ui::modal(r));
    }
    // Always a stack (even with nothing open): see App::view — a changing shape would start the
    // feed's scroll over at the newest message.
    iced::widget::Stack::with_children(layers).into()
}

/// "X is typing…" under the message box (ChatPage.tsx's typing bar): always its height, so the
/// box doesn't jump when someone starts or stops.
fn typing_bar<'a>(who: &[(i64, String, std::time::Instant)], clock: u64) -> El<'a> {
    let line: El = match who {
        [] => space().into(),
        _ => {
            let (names, verb) = match who {
                [a] => (a.1.clone(), "is typing…"),
                [a, b] => (format!("{} and {}", a.1, b.1), "are typing…"),
                _ => ("Several people".to_string(), "are typing…"),
            };
            // Three dots, one lit at a time (the clock ticks every second while anyone types).
            let lit = (clock / 1000 % 3) as usize;
            let dots = (0..3).fold(row![].spacing(3), |r, i| r.push(text("•").size(11).color(if i == lit { hex(0xc7d2fe) } else { FAINT })));
            row![dots, text(names).size(12).font(SEMIBOLD).color(hex(0xe0e7ff)), text(verb).size(12).color(hex(0x949ba4))].spacing(5).align_y(Alignment::Center).into()
        }
    };
    iced::widget::container(line).height(22).padding(iced::Padding { top: 0.0, right: 28.0, bottom: 4.0, left: 28.0 }).align_y(iced::alignment::Vertical::Center).into()
}

/// The feed, with the emoji / GIF pop-up over its bottom-right corner when open.
fn feed_with_panel<'a>(state: &'a State, feed: El<'a>) -> El<'a> {
    match &state.panel {
        Some(panel) => iced::widget::stack![
            feed,
            iced::widget::container(media_picker::view(panel, &state.media, &state.emoji_ui, state.modules, state.selected_room().is_some_and(|r| crate::store::room_permissions::effective(r, state.me_id, state.is_admin).contains(&crate::store::room_permissions::Perm::ManageRoom))))
                .width(Fill)
                .height(Fill)
                .align_x(if panel.on_left() { iced::alignment::Horizontal::Left } else { iced::alignment::Horizontal::Right })
                .align_y(iced::alignment::Vertical::Bottom)
                .padding(iced::Padding { top: 8.0, right: 24.0, bottom: 6.0, left: 20.0 }),
        ]
        .into(),
        // A stack with nothing over the feed: the same shape as with the pop-up, so opening it
        // doesn't move the feed.
        None => iced::widget::stack![feed, space()].into(),
    }
}

/// A picture full size: the larger decode once it's ready, the feed's until then.
fn lightbox(state: &State, id: i64) -> El<'_> {
    use iced::widget::{container, image};
    let name = chat_actions::find(state, id).and_then(|m| m.attachment_data.as_ref()).and_then(|a| a.get("file_name")).and_then(|v| v.as_str()).unwrap_or("image").to_string();
    let frames = state.media.frames(&format!("big:{id}")).or_else(|| state.media.frames(&format!("msg:{id}")));
    let pic: El = match frames {
        Some(f) => image(f.at(state.clock).clone()).content_fit(iced::ContentFit::ScaleDown).width(Fill).height(Fill).into(),
        None => center(text("Loading…").size(13).color(MUTED)).into(),
    };
    let bar = row![
        text(name).size(13).font(SEMIBOLD).color(Color::WHITE).width(Fill),
        button(row![icon(icons::download(), 14, Color::WHITE), text("Save").size(12.5).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center)).padding([6, 12]).style(primary).on_press(msg(ChatMsg::SaveAttachment(id))),
        button(icon(icons::x(), 18, Color::WHITE)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.1))).on_press(msg(ChatMsg::CloseImage)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    iced::widget::mouse_area(
        container(column![bar, iced::widget::opaque(container(pic).width(Fill).height(Fill))].spacing(12))
            .width(Fill)
            .height(Fill)
            .padding(28)
            .style(|_| iced::widget::container::Style { background: Some(rgba(0x000000, 0.88).into()), ..Default::default() }),
    )
    .on_press(msg(ChatMsg::CloseImage))
    .into()
}

fn empty_state(state: &State) -> El<'_> {
    let first = |kind| state.visible_rooms(kind).first().map(|r| r.room_key.clone());
    let public = first(RoomKind::Public);
    center(
        column![
            icon(icons::message_square(), 44, FAINT),
            space().height(6),
            text("No chat open").size(18).font(BOLD).color(Color::WHITE),
            text(if state.rooms.loaded { "Pick a conversation, join a public room or start a new chat." } else { "Loading your chats…" })
                .size(13)
                .color(TEXT_SECONDARY)
                .width(340)
                .align_x(Horizontal::Center),
            space().height(10),
            row![
                button(text("Public rooms").size(13).font(SEMIBOLD)).padding([9, 16]).style(secondary).on_press(match public {
                    Some(k) => msg(ChatMsg::OpenRoom(k)),
                    None => msg(ChatMsg::OpenBrowse),
                }),
                button(text("New chat").size(13).font(SEMIBOLD)).padding([9, 16]).style(primary).on_press(msg(ChatMsg::OpenNewChat(Kind::Dm))),
            ]
            .spacing(8),
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .style(panel(MAIN))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording(kind: crate::recorder::Kind) -> crate::recorder::Recording {
        crate::recorder::Recording {
            kind,
            bytes: vec![0u8; 2048],
            mime: if kind == crate::recorder::Kind::Note { "video/mp4" } else { "audio/mp4" },
            file_name: "VoiceMessage_10-00-00.m4a".into(),
            duration: 7,
            waveform: vec![20; crate::recorder::WAVEFORM_BARS],
            thumbnail: Some("data:image/jpeg;base64,AAAA".into()),
        }
    }

    #[test]
    fn a_voice_message_carries_its_waveform() {
        let data = recording_data(&recording(crate::recorder::Kind::Voice));
        assert_eq!(data["file_type"], "audio/mp4");
        assert_eq!(data["file_size"], 2048);
        assert_eq!(data["duration"], 7);
        assert!(data["file_data"].as_str().is_some_and(|d| d.starts_with("data:audio/mp4;base64,")));
        assert_eq!(data["waveform"].as_array().map(Vec::len), Some(crate::recorder::WAVEFORM_BARS));
        assert!(data.get("is_round").is_none());
        assert!(data.get("thumbnail").is_none());
    }

    #[test]
    fn a_video_note_is_round_and_has_its_first_frame() {
        let data = recording_data(&recording(crate::recorder::Kind::Note));
        assert_eq!(data["file_type"], "video/mp4");
        assert_eq!(data["is_round"], true);
        assert_eq!(data["thumbnail"], "data:image/jpeg;base64,AAAA");
        assert!(data.get("waveform").is_none());
    }

    fn idle_handle() -> iced::task::Handle {
        iced::Task::<()>::none().abortable().1
    }

    fn recording_now(kind: crate::recorder::Kind, with_frame: bool) -> Recording {
        let side = crate::player::FRAME;
        Recording {
            kind,
            started: std::time::Instant::now(),
            level: 50.0,
            bars: vec![40.0; 5],
            frame: with_frame.then(|| iced::widget::image::Handle::from_rgba(side, side, vec![0u8; (side * side * 4) as usize])),
            file: std::path::PathBuf::from("/run/user/1000/accordo-media/recording-1.mp4"),
            _stop: idle_handle(),
        }
    }

    #[test]
    fn the_recording_views_build() {
        let voice = recording_now(crate::recorder::Kind::Voice, false);
        assert_eq!(voice.limit(), crate::recorder::MAX_VOICE_SECS);
        let _ = crate::components::chat::recorder_ui::bar(&voice);

        // A video note before the first frame arrives (bars are empty, no picture yet).
        let mut note = recording_now(crate::recorder::Kind::Note, false);
        note.bars.clear();
        let _ = crate::components::chat::recorder_ui::modal(&note);
        note.frame = recording_now(crate::recorder::Kind::Note, true).frame;
        assert!(note.note());
        let _ = crate::components::chat::recorder_ui::modal(&note);
    }

    #[test]
    fn the_message_box_offers_the_mic_and_the_camera_when_it_is_empty() {
        let room = ChatRoomItem { room_key: "r".into(), chat_type: "dm".into(), title: "Ann".into(), is_member: Some(true), ..Default::default() };
        // Empty: the mic and the camera; written text: the send button; recording: the red bar.
        let _ = composer::view(&room, "", false, None, composer::Mode::Write, None, None, None, None, true);
        let _ = composer::view(&room, "hi", false, None, composer::Mode::Write, None, None, None, None, true);
        let voice = recording_now(crate::recorder::Kind::Voice, false);
        let _ = composer::view(&room, "", false, None, composer::Mode::Write, None, None, None, Some(&voice), true);
    }
}
