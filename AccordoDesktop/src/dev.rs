// SPDX-License-Identifier: GPL-3.0-or-later
//! `--features dev-screenshot`: saves the window as a PNG and quits, to check the look without a
//! screen. ACCORDO_SCREENSHOT=<file.png>; optional ACCORDO_DEMO_USER=<name> skips sign-in with a
//! fake session (networks come from ACCORDO_API_URL or accordo.me) and ACCORDO_OPEN=<room key>
//! opens a room; ACCORDO_PAGE=settings|cheques|invoices|drops|crowdfund (ACCORDO_TAB=launch), ACCORDO_LAYER=wallet|keybackup|chains|contacts|joinlink|joinrequest|teaser|avatar (with ACCORDO_PAGE=settings), ACCORDO_SCROLL=end,
//! ACCORDO_SIZE=<w>x<h> (main.rs); ACCORDO_TOAST=<text>.

use std::time::Duration;

use iced::{window, Task};

use crate::api::auth::{Session, User};
use crate::app::{App, Message, SessionMsg, DataMsg};
use crate::components::toast::Kind;
use crate::pages::chat_page::ChatMsg;

pub fn boot() -> Task<Message> {
    // ACCORDO_SELFTEST=1 with the real saved session: after a few seconds prints counts only
    // (rooms, unread, messages of the first public room, socket) — no content — and quits.
    if std::env::var_os("ACCORDO_SELFTEST").is_some() {
        return Task::perform(tokio::time::sleep(Duration::from_secs(4)), |_| Message::Shoot);
    }
    if std::env::var_os("ACCORDO_SCREENSHOT").is_none() {
        return Task::none();
    }
    // ACCORDO_LOGIN_SCREEN=register|forgot: that form instead of signing in.
    let screen = match std::env::var("ACCORDO_LOGIN_SCREEN").as_deref() {
        Ok("register") => Task::done(Message::Login(crate::pages::login::LoginMsg::Register)),
        Ok("forgot") => Task::done(Message::Login(crate::pages::login::LoginMsg::ForgotPassword)),
        _ => Task::none(),
    };
    Task::batch([screen, Task::perform(tokio::time::sleep(Duration::from_millis(1500)), |_| Message::Shoot)])
}

pub fn update(app: &mut App, message: Message) -> Task<Message> {
    if std::env::var_os("ACCORDO_SELFTEST").is_some() {
        return selftest(app);
    }
    match message {
        Message::Shoot if app.session.is_none() && std::env::var_os("ACCORDO_DEMO_USER").is_some() || std::env::var_os("ACCORDO_TOAST").is_some() && app.toasts.is_empty() => {
            let mut tasks = vec![];
            if let Ok(name) = std::env::var("ACCORDO_DEMO_USER") {
                let base = std::env::var("ACCORDO_API_URL").unwrap_or_else(|_| crate::api::client::DEFAULT_API_URL.into());
                let mut user = User { username: name.clone(), email: Some(format!("{name}@accordo.me")), e2ee_public_key: Some("demo".into()), ..Default::default() };
                // A made-up organization, on the profile and in its Settings card.
                user.orgs = vec![crate::api::orgs::OrgBrief { id: 1, name: "Northwind Labs".into(), verified: true, title: "Founder".into(), ..Default::default() }];
                // ACCORDO_LAYER=orgs|neworg: made-up organizations on the rail; one's window, or the new one.
                if let Ok(l @ ("orgs" | "neworg")) = std::env::var("ACCORDO_LAYER").as_deref() {
                    use crate::api::orgs::{MyOrgs, OrgCard, OrgMember};
                    let mem = |id: i64, u: &str, t: &str, owner: bool| OrgMember { user_id: id, username: u.into(), title: t.into(), owner, ..Default::default() };
                    let a = OrgCard { id: 1, name: "Northwind Labs".into(), verified: true, owner_id: 0, max_members: 10, description: "Payments infrastructure for small shops".into(), website: Some("https://northwind.example".into()), members: vec![mem(0, &name, "Founder", true), mem(2, "alina", "CTO", false), mem(3, "bob", "Support", false)], ..Default::default() };
                    let b = OrgCard { id: 2, name: "Orbit DAO".into(), owner_id: 7, max_members: 10, members: vec![mem(7, "zed", "", true), mem(0, &name, "Advisor", false)], ..Default::default() };
                    tasks.push(Task::done(Message::Org(crate::app::OrgMsg::Loaded(Ok(MyOrgs { orgs: vec![a, b], can_create: true, plan: "enterprise".into() })))));
                    tasks.push(Task::done(Message::Org(if l == "orgs" { crate::app::OrgMsg::Open(1) } else { crate::app::OrgMsg::New })));
                }
                // Made-up limits for the Account Profile card.
                user.private_groups_limit = Some(50);
                user.public_rooms_limit = Some(1);
                user.mini_apps_limit = Some(10);
                // ACCORDO_LAYER=avatar: a made-up picture (a gradient), the editor open on Settings.
                if std::env::var("ACCORDO_LAYER").as_deref() == Ok("avatar") {
                    let img = image::RgbImage::from_fn(160, 160, |x, y| image::Rgb([(x * 255 / 160) as u8, 90, (y * 255 / 160) as u8]));
                    let mut png = Vec::new();
                    let _ = image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png);
                    user.avatar = Some(crate::media::data_url("image/png", &png));
                    tasks.push(Task::done(Message::Avatar(crate::app::AvatarMsg::Edit)));
                }
                tasks.push(app.update(Message::Session(SessionMsg::Restored(Ok(Box::new(Session { base, token: String::new(), user }))))));
                if let Ok(key) = std::env::var("ACCORDO_OPEN") {
                    tasks.push(Task::done(Message::Chat(ChatMsg::OpenRoom(key))));
                }
                use crate::app::Nav;
                let page = match std::env::var("ACCORDO_PAGE").as_deref() {
                    Ok("settings") => Some(Nav::Settings),
                    Ok("cheques") => Some(Nav::Cheques),
                    Ok("invoices") => Some(Nav::Invoices),
                    Ok("drops") => Some(Nav::Drops),
                    Ok("crowdfund") => Some(Nav::Crowdfund),
                    _ => None,
                };
                if let Some(nav) = page {
                    tasks.push(Task::done(Message::Navigate(nav)));
                }
                // ACCORDO_TAB=launch (crowdfund): the Launch tab, its form filled in a little.
                #[cfg(feature = "crowdfund")]
                if std::env::var("ACCORDO_TAB").as_deref() == Ok("launch") {
                    use crate::pages::crowdfund_page::{CrowdfundMsg, CrowdfundTab};
                    for m in [CrowdfundMsg::Tab(CrowdfundTab::Launch), CrowdfundMsg::Title("Community garden".into()), CrowdfundMsg::Category(5), CrowdfundMsg::Target("500".into()), CrowdfundMsg::Description("Seeds, tools and a water tank for the courtyard garden.".into())] {
                        tasks.push(Task::done(Message::Crowdfund(m)));
                    }
                }
                // ACCORDO_TAB=swap (cheques): the write form in Swap mode, its price filled in.
                if std::env::var("ACCORDO_TAB").as_deref() == Ok("swap") {
                    use crate::pages::cheques_page::ChequesMsg as C;
                    for m in [C::Swap(true), C::Amount("250".into()), C::GetAmount("0.08".into())] {
                        tasks.push(Task::done(Message::Cheques(m)));
                    }
                }
                // ACCORDO_TAB=newdrop (drops): a new drop paid in parts, two plans, a wallet on each, and
                // the second plan's unlock share half typed ("12.").
                #[cfg(feature = "drops")]
                if std::env::var("ACCORDO_TAB").as_deref() == Ok("newdrop") {
                    use crate::pages::drops_page::DropsMsg as D;
                    for m in [
                        D::ToggleNew,
                        D::Parts(true),
                        D::RowAddress(0, "0x70997970C51812dc3A010C7d01b50e0d17dc79C8".into()),
                        D::RowAmount(0, "100".into()),
                        D::AddRow,
                        D::RowAddress(1, "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC".into()),
                        D::RowAmount(1, "250".into()),
                        D::PlanAdd,
                        D::PlanName(2, "Investors".into()),
                        D::PlanUpfront(2, "12.".into()),
                        D::RowPlan(1, 2),
                    ] {
                        tasks.push(Task::done(Message::Drops(m)));
                    }
                }
                // ACCORDO_DEVTOOLS_OPEN=network|socket|console: the F12 panel on that tab, newest entry selected.
                if let Ok(tab) = std::env::var("ACCORDO_DEVTOOLS_OPEN") {
                    use crate::components::devtools_panel::{DevMsg, Tab};
                    let tab = match tab.as_str() {
                        "socket" => Tab::Socket,
                        "console" => Tab::Console,
                        _ => Tab::Network,
                    };
                    tasks.push(Task::batch([Task::done(Message::Dev(DevMsg::Toggle)), Task::done(Message::Dev(DevMsg::Tab(tab)))]));
                    tasks.push(Task::perform(tokio::time::sleep(Duration::from_millis(1200)), |_| ()).then(|_| {
                        let last = crate::devtools::snapshot().iter().rev().find(|e| matches!(e.kind, crate::devtools::Kind::Net(_))).map(|e| e.id);
                        last.map_or(Task::none(), |id| Task::done(Message::Dev(crate::components::devtools_panel::DevMsg::Select(id))))
                    }));
                }
                // ACCORDO_SCROLL=end: the bottom of the settings page.
                if std::env::var("ACCORDO_SCROLL").as_deref() == Ok("end") {
                    tasks.push(
                        Task::perform(tokio::time::sleep(Duration::from_millis(2500)), |_| ())
                            .then(|_| iced::widget::operation::snap_to_end(crate::pages::settings_page::SCROLL_ID)),
                    );
                }
                match std::env::var("ACCORDO_LAYER").as_deref() {
                    Ok("wallet") => tasks.push(Task::done(Message::OpenWallet)),
                    // The wallet drawer, unlocked with a demo password: a demo wallet added, then the
                    // list ("walletlist") or the Generate form ("walletadd").
                    Ok(l @ ("walletlist" | "walletadd")) => {
                        use crate::components::wallet_panel::WalletMsg as W;
                        use crate::pages::settings_page::SettingsMsg as S;
                        let pw = "demo password 123";
                        tasks.push(Task::batch([S::VaultValue(pw.into()), S::VaultConfirm(pw.into()), S::VaultSave].map(|m| Task::done(Message::Settings(m)))));
                        let add = l == "walletadd";
                        tasks.push(Task::perform(tokio::time::sleep(Duration::from_millis(7000)), |_| ()).then(move |_| {
                            let mut steps = vec![Message::OpenWallet, Message::Wallet(W::StartAdd), Message::Wallet(W::Save)];
                            if add {
                                steps.push(Message::Wallet(W::StartAdd));
                            }
                            Task::batch(steps.into_iter().map(Task::done))
                        }));
                    }
                    // The master-password card: a generated password / phrase, or unlocked (demo password).
                    Ok(l @ ("vaultgen" | "vaultphrase" | "vaultunlocked")) => {
                        use crate::components::key_backup_modal::Tab;
                        use crate::pages::settings_page::SettingsMsg as S;
                        let steps: Vec<S> = match l {
                            "vaultgen" => vec![S::VaultGenerate],
                            "vaultphrase" => vec![S::VaultTab(Tab::SeedPhrase), S::VaultGenerate, S::VaultSavedIt(true)],
                            _ => vec![S::VaultValue("demo password 123".into()), S::VaultConfirm("demo password 123".into()), S::VaultSave],
                        };
                        tasks.extend(steps.into_iter().map(|m| Task::done(Message::Settings(m))));
                        tasks.push(
                            Task::perform(tokio::time::sleep(Duration::from_millis(5000)), |_| ())
                                .then(|_| iced::widget::operation::snap_to(crate::pages::settings_page::SCROLL_ID, iced::widget::operation::RelativeOffset { x: 0.0, y: 0.62 })),
                        );
                    }
                    // A made-up group card (Suggested, not joined): "Ask to join".
                    Ok("teaser") => {
                        use crate::api::chat::{ChatRoomItem, RoomTeaser};
                        let at = (chrono::Utc::now() - chrono::Duration::hours(8)).format("%Y-%m-%dT%H:%M:%S").to_string();
                        let teaser = RoomTeaser { members: 12, online: 3, last_activity: Some(at), ..Default::default() };
                        let room = ChatRoomItem { room_key: "group:teaser".into(), chat_type: "group".into(), title: "Ghost Secret Room".into(), topic: Some("Private OTC desk — verified traders only".into()), is_teaser: Some(true), teaser: Some(teaser), ..Default::default() };
                        app.chat.rooms.rooms.push(room);
                        app.chat.rooms.loaded = true;
                        tasks.push(Task::done(Message::Chat(ChatMsg::OpenRoom("group:teaser".into()))));
                    }
                    // "Join with a link": the paste step, or (made-up room, no network) the request.
                    Ok(l @ ("joinlink" | "joinrequest")) => {
                        use crate::components::chat::join_invite::JoinMsg as J;
                        let j = |m: J| Task::done(Message::Chat(ChatMsg::JoinInvite(m)));
                        tasks.push(j(J::Open));
                        tasks.push(j(J::Link("https://accordo.me/chat?invite=Xy7_demo#k=demo".into())));
                        if l == "joinrequest" {
                            let room = crate::api::chat::ChatRoomItem { room_key: "group:demo".into(), chat_type: "group".into(), title: "Ghost Secret Room".into(), status: Some("pending".into()), ..Default::default() };
                            tasks.push(j(J::Joined(Ok(Box::new(room)))));
                        }
                    }
                    Ok("newchat") => tasks.push(Task::done(Message::Chat(crate::pages::chat_page::ChatMsg::OpenNewChat(crate::components::chat::new_chat_modal::Kind::Dm)))),
                    Ok("newgroup") => {
                        use crate::components::chat::new_chat_modal::NewChatMsg;
                        use crate::pages::chat_page::ChatMsg;
                        tasks.push(Task::done(Message::Chat(ChatMsg::OpenNewChat(crate::components::chat::new_chat_modal::Kind::Group))));
                        tasks.push(Task::done(Message::Chat(ChatMsg::NewChat(NewChatMsg::ToggleAdvanced))));
                    }
                    Ok(l @ ("manage" | "members" | "settingstab")) => {
                        // A made-up group (no real data): the Manage Room window on it.
                        use crate::api::chat::{ChatRoomItem, UserInfo};
                        use crate::components::chat::manage_room::Tab;
                        let me = UserInfo { id: 0, username: name.clone(), display_name: name.clone(), ..Default::default() };
                        let alina = UserInfo { id: 2, username: "alina".into(), display_name: "Alina".into(), ..Default::default() };
                        let bob = UserInfo { id: 3, username: "bob".into(), ..Default::default() };
                        let room = ChatRoomItem {
                            room_key: "group:demo".into(),
                            chat_type: "group".into(),
                            title: "OTC Alpha Desk".into(),
                            topic: Some("Escrow and settlement".into()),
                            created_by: Some(0),
                            allowed_user_ids: vec![0, 2, 3],
                            allowed_users: vec![me, alina, bob],
                            join_requests_count: Some(2),
                            settings: Some(serde_json::from_value(serde_json::json!({"roles": [{"id": "r1", "name": "Trader", "color": "#38bdf8"}], "member_roles": {"2": "r1"}, "slow_mode_seconds": 10})).unwrap_or_default()),
                            ..Default::default()
                        };
                        app.chat.rooms.rooms.push(room);
                        app.chat.selected = Some("group:demo".into());
                        app.chat.pending_admissions.insert("group:demo".into(), vec![3]);
                        let tab = match l { "members" => Tab::Members, "settingstab" => Tab::Settings, _ => Tab::General };
                        tasks.push(Task::done(Message::Chat(ChatMsg::OpenManage(tab))));
                    }
                    Ok(l @ ("feed" | "menu" | "thread" | "reply" | "memberlist" | "pinned" | "membermenu" | "profile" | "profileroom" | "profilekey" | "media" | "plus" | "time" | "emoji" | "react" | "cheque" | "chequehist" | "drop")) => {
                        // A made-up public room with made-up messages (no real data), delivered
                        // after the chat key settled (that reloads feeds).
                        use crate::api::chat::{ChatMessageItem, ChatRoomItem, ThreadSummary};
                        use crate::components::chat::message_list::MenuKind;
                        use crate::pages::chat_actions::ActionResult;
                        let at = |min: i64| (chrono::Utc::now() - chrono::Duration::minutes(min)).format("%Y-%m-%dT%H:%M:%S").to_string();
                        let said = |id: i64, who: &str, uid: i64, text: &str, min: i64| ChatMessageItem { id, room_id: "demo".into(), sender_id: Some(uid), sender_username: who.into(), content: text.into(), message_type: "text".into(), created_at: at(min), ..Default::default() };
                        let mut a = said(1, "alina", 2, "Escrow is funded, can you check the invoice? 🔥", 40);
                        a.reactions = Some([("🔥".to_string(), vec![0, 2]), ("👍".to_string(), vec![3])].into());
                        a.thread_summary = Some(ThreadSummary { count: 2, last_reply_at: Some(at(5)), participant_ids: vec![0, 3] });
                        let b = said(2, "alina", 2, "Network is Base, 1,200 USDC", 39);
                        let mut c = said(3, &name, 0, "Checked — looks right. Releasing now.", 12);
                        c.is_edited = Some(true);
                        c.is_pinned = Some(true);
                        let mut d = said(4, &name, 0, "Done", 11);
                        d.reply_to_id = Some(2);
                        d.reply_to_data = Some(serde_json::json!({"id": 2, "sender_username": "alina", "sender_display_name": "alina", "content": "Network is Base, 1,200 USDC", "message_type": "text"}));
                        use crate::api::chat::UserInfo;
                        let users = vec![
                            UserInfo { id: 0, username: name.clone(), display_name: name.clone(), is_online: Some(true), ..Default::default() },
                            UserInfo { id: 2, username: "alina".into(), display_name: "Alina".into(), is_online: Some(true), verified: Some(true), created_at: Some(at(60 * 24 * 200)), e2ee_public_key: Some(r#"{"kty":"EC","crv":"P-256","x":"f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU","y":"x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0"}"#.into()), wallets: Some(vec![crate::api::auth::PublicWallet { name: Some("Main".into()), address: "0x9a3f1c22b7e04d5a8c6e1f0b2d3c4e5f6a7b8c9d".into(), chain: Some("evm".into()), is_default: Some(true), id: None }]), ..Default::default() },
                            UserInfo { id: 3, username: "bob".into(), last_seen_at: Some(at(25)), ..Default::default() },
                        ];
                        let room = ChatRoomItem { room_key: "demo".into(), chat_type: "group".into(), title: "OTC desk".into(), is_member: Some(true), created_by: Some(0), allowed_user_ids: vec![0, 2, 3], allowed_users: users, settings: Some(serde_json::from_value(serde_json::json!({"roles": [{"id": "r1", "name": "Trader", "color": "#38bdf8"}], "member_roles": {"2": "r1"}})).unwrap_or_default()), ..Default::default() };
                        app.chat.rooms.rooms.push(room);
                        app.chat.selected = Some("demo".into());
                        let mut feed = vec![a.clone(), b.clone(), c, d];
                        if matches!(l, "media" | "plus" | "time") {
                            // A made-up picture (a gradient), a file and a countdown.
                            let mut img = image::RgbaImage::new(480, 300);
                            for (x, y, p) in img.enumerate_pixels_mut() {
                                *p = image::Rgba([(x * 255 / 480) as u8, 90, (y * 255 / 300) as u8, 255]);
                            }
                            let mut png = Vec::new();
                            image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok();
                            let mut pic = said(5, "alina", 2, "", 4);
                            pic.message_type = "image".into();
                            pic.attachment_data = Some(serde_json::json!({"file_name": "escrow.png", "file_size": png.len(), "file_type": "image/png", "file_data": crate::media::data_url("image/png", &png)}));
                            let mut file = said(6, &name, 0, "Signed contract", 3);
                            file.message_type = "file".into();
                            file.attachment_data = Some(serde_json::json!({"file_name": "otc-agreement.pdf", "file_size": 248_133, "file_type": "application/pdf", "file_data": "data:application/pdf;base64,JVBERi0="}));
                            let when = chrono::Utc::now().timestamp() + 16 * 3600 + 3 * 60;
                            let timer = said(7, &name, 0, &format!("Settlement window closes <t:{when}:C>"), 1);
                            // A voice message and a video note (made-up: a waveform, a gradient first frame).
                            let mut voice = said(9, "alina", 2, "🎤 Voice message (0:07)", 1);
                            voice.message_type = "audio".into();
                            let wave: Vec<u32> = (0..32).map(|i| (40.0 + 35.0 * ((i as f32) * 0.7).sin().abs()) as u32).collect();
                            voice.attachment_data = Some(serde_json::json!({"file_name": "voice.webm", "file_size": 9000, "file_type": "audio/webm", "duration": 7, "waveform": wave, "file_data": "data:audio/webm;base64,GkXfow=="}));
                            let thumb = image::RgbImage::from_fn(240, 240, |x, y| image::Rgb([(60 + x / 3) as u8, (40 + y / 4) as u8, 140]));
                            let mut jpg = Vec::new();
                            image::DynamicImage::ImageRgb8(thumb).write_to(&mut std::io::Cursor::new(&mut jpg), image::ImageFormat::Jpeg).ok();
                            let mut note = said(10, &name, 0, "📹 Video message (5s)", 0);
                            note.message_type = "video".into();
                            note.attachment_data = Some(serde_json::json!({"file_name": "VideoNote.mp4", "file_size": 431_100, "file_type": "video/mp4", "duration": 5, "is_round": true,
                                "thumbnail": crate::media::data_url("image/jpeg", &jpg), "file_data": "data:video/mp4;base64,AAAA"}));
                            feed.extend([pic, file, timer, voice, note]);
                        }
                        if matches!(l, "cheque" | "chequehist") {
                            // A made-up cheque, shaped like the server's.
                            let exp = chrono::Utc::now().timestamp() + 28 * 86400 + 324;
                            let mut chq = said(8, &name, 0, "", 2);
                            chq.message_type = "cheque".into();
                            chq.attachment_data = Some(serde_json::json!({
                                "cheque_id": "0x83c8706563f09491a37804c44b7f97472e139b6e671690000000000000000001",
                                "status": "pending",
                                "cheque_owner_identificator": name,
                                "created_at": "2026-11-13T18:00:00",
                                "detail": {"chain": "Anvil (local)", "token_symbol": "ETH", "original_amount": "10", "fee_bps": 500, "expires_at": exp,
                                    "from": "0x3282490000000000000000000000000000573162", "to": ["0x3282490000000000000000000000000000573162"],
                                    "claims": {"slot1": {"claimed": false}}}
                            }));
                            feed.push(chq);
                            app.chat.my_wallets = vec![("0x3282490000000000000000000000000000573162".into(), "Wallet 1 (EVM)".into())];
                        }
                        // ACCORDO_LAYER=drop: a made-up drop, its card as if the contract had answered — a
                        // share vesting monthly after a month's cliff, the upfront part claimed.
                        #[cfg(feature = "drops")]
                        if l == "drop" {
                            use crate::api::drops::{DropInfo, DropSlot, SlotSchedule};
                            use crate::components::chat::fund_cards::DropCard;
                            let now = chrono::Utc::now().timestamp().max(0) as u64;
                            let me = "0x3282490000000000000000000000000000573162";
                            let id = format!("0x{}", "d7".repeat(32));
                            let mut m = said(8, "alina", 2, "", 2);
                            m.message_type = "drop".into();
                            m.attachment_data = Some(serde_json::json!({"type": "drop", "drop_id": id, "chain": "Base", "title": "Core team allocation", "token": "USDC"}));
                            feed.push(m);
                            let month = 2_629_800u32;
                            let info = DropInfo { drop_id: id.clone(), chain: "Base".into(), creator_address: "0x9a3f1c22b7e04d5a8c6e1f0b2d3c4e5f6a7b8c9d".into(), title: Some("Core team allocation".into()),
                                note: Some("Vests monthly after a one-month cliff. Thanks for building with us!".into()), token: "USDC".into(), token_decimals: 6, total: "1200".into(), total_wei: "1200000000".into(),
                                fee_bps: 50, expires_at: now + 400 * 86_400, status: "active".into(), ..Default::default() };
                            let slot = DropSlot { slot: 3, address: me.into(), amount: "300".into(), amount_wei: "300000000".into(), net_amount: "298.5".into(),
                                schedule: SlotSchedule { start: now - 40 * 86_400, interval: month, periods: 6, cliff: month, upfront_bps: 2_500 }, claimed_amount: "75".into(), claimed_wei: "75000000".into(), ..Default::default() };
                            app.chat.fund.drops.insert(id, DropCard { info: Some(Ok(info)), slots: Some(vec![slot]), chain: Some(Ok(())), ..Default::default() });
                            app.chat.my_wallets = vec![(me.into(), "Wallet 1 (EVM)".into())];
                        }
                        let mut then = vec![Message::Chat(ChatMsg::MessagesLoaded("demo".into(), Ok(feed)))];
                        match l {
                            "menu" => then.push(Message::Chat(ChatMsg::Menu(3, MenuKind::Context))),
                            "reply" => then.push(Message::Chat(ChatMsg::Reply(2))),
                            "memberlist" => then.push(Message::Chat(ChatMsg::ToggleMembers)),
                            "membermenu" => {
                                then.push(Message::Chat(ChatMsg::ToggleMembers));
                                then.push(Message::Chat(ChatMsg::MemberMenu(2)));
                            }
                            "pinned" => then.push(Message::Chat(ChatMsg::OpenPinned)),
                            "plus" => then.push(Message::Chat(ChatMsg::OpenPlus)),
                            "chequehist" => {
                                use crate::components::chat::cheque_card::{ChequeMsg, Tab};
                                let id = "0x83c8706563f09491a37804c44b7f97472e139b6e671690000000000000000001".to_string();
                                then.push(Message::Chat(ChatMsg::Cheque(ChequeMsg::Tab(id.clone(), Tab::History))));
                                then.push(Message::Chat(ChatMsg::Cheque(ChequeMsg::Claims(id, Ok(vec![serde_json::json!({"action_type": "CLAIM", "amount": "9.5", "token_symbol": "ETH", "claimant_identifier": "alina", "claimant_address": "0x9a3f1c22b7e04d5a8c6e1f0b2d3c4e5f6a7b8c9d", "tx_hash": "0x5f2e8a0c9b7d6e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f", "chain": "Base", "created_at": "2026-11-14T10:12:00"})])))));
                            }
                            "time" => then.push(Message::TimePicker(crate::components::time_picker::TimeMsg::Open(crate::components::time_picker::Purpose::Chat, None))),
                            "emoji" => then.push(Message::Chat(ChatMsg::OpenEmoji)),
                            "react" => then.push(Message::Chat(ChatMsg::Menu(3, MenuKind::React))),
                            "profile" | "profileroom" | "profilekey" => {
                                let alina = app.chat.rooms.rooms.last().and_then(|r| r.allowed_users.get(1).cloned()).unwrap_or_default();
                                then.push(Message::Chat(ChatMsg::OpenProfile(Box::new(alina))));
                                let tab = match l {
                                    "profileroom" => Some(crate::components::chat::user_profile::Tab::Room),
                                    "profilekey" => Some(crate::components::chat::user_profile::Tab::Security),
                                    _ => None,
                                };
                                if let Some(t) = tab {
                                    then.push(Message::Chat(ChatMsg::Profile(crate::components::chat::user_profile::ProfileMsg::Tab(t))));
                                }
                            }
                            "thread" => {
                                let mut r1 = said(10, "bob", 3, "Same here, all good", 8);
                                r1.thread_root_id = Some(1);
                                let mut r2 = said(11, &name, 0, "shipping 🚀", 5);
                                r2.thread_root_id = Some(1);
                                let data = crate::api::chat::threads::Thread { root: a, replies: vec![r1, r2] };
                                then.push(Message::Chat(ChatMsg::OpenThread(1)));
                                tasks.push(Task::perform(tokio::time::sleep(Duration::from_millis(1500)), move |_| Message::Chat(ChatMsg::Action(Box::new(ActionResult::ThreadLoaded(1, Ok(Box::new(data.clone()))))))));
                            }
                            _ => {}
                        }
                        tasks.push(Task::perform(tokio::time::sleep(Duration::from_millis(900)), |_| ()).then(move |_| Task::batch(then.clone().into_iter().map(Task::done))));
                    }
                    Ok("picker") => tasks.push(Task::done(Message::Picker(crate::components::file_picker::PickerMsg::Open(crate::components::file_picker::Purpose::ChatKeyFile)))),
                    Ok("restore") => tasks.push(Task::done(Message::Settings(crate::pages::settings_page::SettingsMsg::OpenKeyBackup(crate::components::key_backup_modal::Mode::Restore)))),
                    // ACCORDO_LAYER=appsettings|appprivacy|appshortcuts|appappearance|appabout: the App settings window on that tab.
                    Ok(l @ ("appsettings" | "appprivacy" | "appshortcuts" | "appappearance" | "appabout")) => {
                        use crate::components::app_settings_modal::{AppSettingsMsg as A, Tab};
                        let tab = match l {
                            "appprivacy" => Tab::Privacy,
                            "appshortcuts" => Tab::Shortcuts,
                            "appappearance" => Tab::Appearance,
                            "appabout" => Tab::About,
                            _ => Tab::General,
                        };
                        tasks.push(Task::batch([Task::done(Message::AppSettings(A::Open)), Task::done(Message::AppSettings(A::Tab(tab)))]));
                    }
                    // ACCORDO_LAYER=appshortcutsrec: Shortcuts, "Next unread chat" waiting for its keys, and
                    // Ctrl+C pressed (refused: it's for text) — the row's note.
                    Ok("appshortcutsrec") => {
                        use crate::components::app_settings_modal::{AppSettingsMsg as A, Tab};
                        tasks.push(Task::batch([Task::done(Message::AppSettings(A::Open)), Task::done(Message::AppSettings(A::Tab(Tab::Shortcuts))), Task::done(Message::AppSettings(A::RecordKey(crate::hotkeys::Action::NextUnread)))]));
                        tasks.push(Task::perform(tokio::time::sleep(Duration::from_millis(800)), |_| ()).then(|_| {
                            window::latest().and_then(|window| {
                                Task::done(Message::Key { key: iced::keyboard::Key::Character("c".into()), physical: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyC), modifiers: iced::keyboard::Modifiers::CTRL, captured: false, window })
                            })
                        }));
                    }
                    // Ctrl+K pressed in the main window (the only one open at this point).
                    // ACCORDO_LAYER=contacts (ACCORDO_PAGE=cheques): @ typed in the first recipient's name —
                    // the contacts window, someone being added.
                    Ok("contacts") => tasks.push(Task::perform(tokio::time::sleep(Duration::from_millis(800)), |_| ()).then(|_| {
                        use crate::components::contact_picker::ContactsMsg as C;
                        let steps = [
                            Message::Cheques(crate::pages::cheques_page::ChequesMsg::RecipientName(0, "@".into())),
                            Message::Contacts(C::Query("Alice".into())),
                            Message::Contacts(C::StartAdd),
                            Message::Contacts(C::Address("0x70997970C51812dc3A010C7d01b50e0d17dc79C8".into())),
                        ];
                        Task::batch(steps.into_iter().map(Task::done))
                    })),
                    Ok("switcher") => tasks.push(Task::perform(tokio::time::sleep(Duration::from_millis(1500)), |_| ()).then(|_| {
                        window::latest().and_then(|window| {
                            Task::done(Message::Key { key: iced::keyboard::Key::Character("k".into()), physical: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyK), modifiers: iced::keyboard::Modifiers::CTRL, captured: false, window })
                        })
                    })),
                    // ACCORDO_LAYER=chains|chainslocal: the Networks window on Base / on the local Anvil
                    // chain (with the faucet), over a demo network list.
                    Ok(l @ ("chains" | "chainslocal")) => {
                        use crate::api::data::Chain;
                        use crate::components::chain_settings::ChainSettingsMsg as C;
                        let c = |id: &str, name: &str, testnet: bool, local: bool, rpc: &[&str], ex: Option<&str>| Chain {
                            id: id.into(),
                            name: name.into(),
                            testnet,
                            native_symbol: if id == "56" || id == "97" { "BNB".into() } else { "ETH".into() },
                            explorer: ex.map(str::to_string),
                            rpc_urls: rpc.iter().map(|u| u.to_string()).collect(),
                            local,
                            solana: false,
                            contracts: Default::default(),
                        };
                        let chains = vec![
                            c("1", "Ethereum", false, false, &["https://eth.llamarpc.com", "https://rpc.ankr.com/eth"], Some("https://etherscan.io")),
                            c("8453", "Base", false, false, &["https://mainnet.base.org", "https://base.llamarpc.com"], Some("https://basescan.org")),
                            c("42161", "Arbitrum One", false, false, &["https://arb1.arbitrum.io/rpc"], Some("https://arbiscan.io")),
                            c("56", "BNB Smart Chain", false, false, &["https://bsc-dataseed.binance.org"], Some("https://bscscan.com")),
                            c("84532", "Base Sepolia", true, false, &["https://sepolia.base.org"], Some("https://sepolia.basescan.org")),
                            c("11155111", "Ethereum Sepolia", true, false, &["https://rpc.sepolia.org"], Some("https://sepolia.etherscan.io")),
                            c("31337", "Anvil (local)", false, true, &["http://127.0.0.1:8545"], None),
                        ];
                        let mut steps = vec![Message::Data(DataMsg::ChainsLoaded(Ok(chains))), Message::ChainSettings(C::Open)];
                        if l == "chainslocal" {
                            steps.push(Message::ChainSettings(C::Pick("31337".into())));
                        } else {
                            steps.push(Message::ChainSettings(C::Rpc(iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(std::sync::Arc::new("https://my-node.example.com/base".into()))))));
                        }
                        tasks.push(Task::batch(steps.into_iter().map(Task::done)));
                    }
                    Ok("hidden") => tasks.push(Task::done(Message::AppSettings(crate::components::app_settings_modal::AppSettingsMsg::HideNow))),
                    Ok("keybackup") => tasks.push(Task::done(Message::Settings(crate::pages::settings_page::SettingsMsg::OpenKeyBackup(crate::components::key_backup_modal::Mode::Create)))),
                    _ => {}
                }
            }
            if let Ok(t) = std::env::var("ACCORDO_TOAST") {
                tasks.push(app.toasts.show(Kind::Error, t));
            }
            tasks.push(Task::perform(tokio::time::sleep(Duration::from_millis(std::env::var("ACCORDO_WAIT_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(1500))), |_| Message::Shoot));
            Task::batch(tasks)
        }
        Message::Shoot => app.main_window.map(|id| window::screenshot(id).map(Message::Shot)).unwrap_or_else(Task::none),
        Message::Shot(shot) => {
            let path = std::env::var("ACCORDO_SCREENSHOT").unwrap_or_else(|_| "screenshot.png".into());
            let file = std::io::BufWriter::new(std::fs::File::create(&path).expect("create screenshot file"));
            let mut encoder = png::Encoder::new(file, shot.size.width, shot.size.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().and_then(|mut w| w.write_image_data(&shot.rgba)).expect("write screenshot");
            iced::exit()
        }
        _ => Task::none(),
    }
}

/// Two rounds: open the first public room, then report.
fn selftest(app: &mut App) -> Task<Message> {
    use crate::components::chat::display::{self, RoomKind};
    static ROUND: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
    let chat = &app.chat;
    if ROUND.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
        let first_public = chat.rooms.rooms.iter().find(|r| display::kind(r) == RoomKind::Public).map(|r| r.room_key.clone());
        let open = first_public.map_or(Task::none(), |k| Task::done(Message::Chat(ChatMsg::OpenRoom(k))));
        return open.chain(Task::perform(tokio::time::sleep(Duration::from_secs(3)), |_| Message::Shoot));
    }
    let count = |kind| chat.rooms.rooms.iter().filter(|r| display::kind(r) == kind).count();
    let feed = chat.selected.as_ref().and_then(|k| chat.messages.get(k)).map(Vec::len);
    let encrypted_previews = chat.rooms.rooms.iter().filter(|r| r.last_message.as_deref().is_some_and(display::looks_encrypted)).count();
    if ROUND.load(std::sync::atomic::Ordering::SeqCst) == 2 {
        let t = Task::done(Message::Chat(ChatMsg::OpenBrowse));
        return t.chain(Task::perform(tokio::time::sleep(Duration::from_secs(3)), |_| Message::Shoot));
    }
    let catalog = chat.browse.as_ref().and_then(|b| b.results.as_ref()).map(|c| (c.total, c.rooms.len()));
    eprintln!("SELFTEST public_catalog(total, listed)={catalog:?}");
    if let Some(s) = &app.session {
        eprintln!("SELFTEST can_create_public_rooms={} is_admin={}", s.user.can_create_public_rooms, s.user.is_admin);
    }
    eprintln!(
        "SELFTEST signed_in={} rooms_loaded={} public={} groups={} dms={} unread_total={} encrypted_previews={} opened_public_messages={:?} socket_live={}",
        app.session.is_some(), chat.rooms.loaded, count(RoomKind::Public), count(RoomKind::Group), count(RoomKind::Dm),
        chat.total_unread(), encrypted_previews, feed, app.socket_live,
    );
    iced::exit()
}
