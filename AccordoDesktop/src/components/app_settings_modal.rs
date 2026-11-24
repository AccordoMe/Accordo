// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings of the app itself (not the account): the gear next to the window buttons, or Ctrl+,.
//! A desktop window with sections on the left, like an editor's preferences: General,
//! Appearance, Privacy, Shortcuts, Modules, Developer, Storage, About. Kept on this computer only.

use iced::widget::{button, column, container, row, scrollable, space, svg, text, toggler, Column, Row};
use iced::{Alignment, Color, Fill, Font, Length, Padding};

use super::overlay;
use super::ui::{icon, El};
use crate::app::Message;
use crate::e2ee::keystore::Lifetime;
use crate::hotkeys;
use crate::icons;
use crate::store::app_store::AppStore;
use crate::usage::{self, Usage};
use crate::theme::{self, *};

pub const SCALES: [f32; 6] = [0.9, 1.0, 1.1, 1.25, 1.5, 1.75];

/// Pages the app can open on (id, label).
pub const START_PAGES: [(&str, &str); 6] = [("chat", "Chat"), ("cheques", "Cheques"), ("invoices", "Invoices"), ("drops", "Drops"), ("crowdfund", "Crowdfund"), ("settings", "Settings")];

/// Storage → Chat key: how long it stays on this computer.
pub const CHAT_KEY_LIFETIMES: [(&str, Lifetime); 5] =
    [("Always", Lifetime::Forever), ("Until the app closes", Lifetime::ThisRun), ("1 day", Lifetime::Days(1)), ("7 days", Lifetime::Days(7)), ("30 days", Lifetime::Days(30))];

/// Storage: the open account's chat key on this computer.
#[derive(Debug, Clone, Copy)]
pub struct ChatKeyInfo {
    /// A key is here now.
    pub here: bool,
    /// The account has a backup on Accordo (the chat password brings a deleted key back).
    pub has_backup: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    General,
    Appearance,
    Privacy,
    Shortcuts,
    Modules,
    Developer,
    Storage,
    About,
}

/// On/off preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pref {
    AutoplayGifs,
    BlurMessages,
    BlurPreviews,
    HideOnBlur,
    LockWalletsOnHide,
    Shortcuts,
}

#[derive(Debug, Clone)]
pub enum AppSettingsMsg {
    Open,
    Close,
    Tab(Tab),
    Scale(f32),
    StartPage(&'static str),
    Toggle(Pref, bool),
    /// A section on (true) or hidden (false).
    Module(crate::modules::Module, bool),
    HideNow,
    DevMode(bool),
    /// The developer tools open (true) or closed — remembered for the next start.
    Devtools(bool),
    OpenDataFolder,
    /// Storage: remove every account's kept wallet unlock from the secret store, and lock now.
    ForgetWalletKeys,
    /// Storage: delete the open account's chat key from this computer (only with a backup).
    ForgetChatKey,
    /// Storage: how long a chat key stays on this computer.
    ChatKeyLifetime(Lifetime),
    /// Shortcuts: wait for this action's new keys (again: stop waiting).
    RecordKey(hotkeys::Action),
    ClearKey(hotkeys::Action),
    ResetKey(hotkeys::Action),
    ResetKeys,
}

pub struct View<'a> {
    pub tab: Tab,
    pub store: &'a AppStore,
    /// The signed-in server, if any.
    pub server: Option<&'a str>,
    pub usage: &'a Usage,
    pub modules: crate::modules::Modules,
    /// The signed-in account is a developer (from the server; None: not signed in).
    pub developer: Option<bool>,
    /// The developer tools are open right now.
    pub devtools_open: bool,
    /// Shortcuts: the action waiting for its new keys, and the word under a row.
    pub recording: Option<hotkeys::Action>,
    pub note: Option<&'a (hotkeys::Action, String)>,
    /// Storage: the open account's wallets are unlocked with a timer — kept until then (ms).
    pub wallet_kept_until: Option<i64>,
    /// Storage: the open account's chat key (None: signed out).
    pub chat_key: Option<ChatKeyInfo>,
}

fn msg(m: AppSettingsMsg) -> Message {
    Message::AppSettings(m)
}

fn section<'a>(title: &'a str, body: El<'a>) -> El<'a> {
    column![text(title).size(11).font(BOLD).color(TEXT_SECONDARY), body].spacing(8).into()
}

/// A labelled row: title, a line of explanation, the control on the right.
fn setting<'a>(label: &'a str, hint: &'a str, control: El<'a>) -> El<'a> {
    container(
        row![
            column![text(label).size(13.5).font(SEMIBOLD).color(TEXT_BODY), text(hint).size(11.5).color(TEXT_SECONDARY).line_height(1.4)].spacing(3).width(Fill),
            control,
        ]
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([11, 14])
    .style(theme::soft_row)
    .into()
}

fn switch_for<'a>(on: bool, pref: Pref) -> El<'a> {
    toggler(on).size(20).style(switch).on_toggle(move |v| msg(AppSettingsMsg::Toggle(pref, v))).into()
}

fn key_chip<'a>(k: impl iced::widget::text::IntoFragment<'a>) -> El<'a> {
    container(text(k).size(11.5).font(Font::MONOSPACE).color(TEXT_BODY))
        .padding([3, 8])
        .style(|_| container::Style {
            background: Some(rgba(0xffffff, 0.06).into()),
            border: border(rgba(0xffffff, 0.14), 1.0, 5.0),
            shadow: iced::Shadow { color: rgba(0x000000, 0.5), offset: iced::Vector::new(0.0, 1.5), blur_radius: 0.0 },
            ..Default::default()
        })
        .into()
}

/// A combination as key caps: Ctrl + Shift + K.
fn combo_chips<'a>(c: &hotkeys::Combo) -> El<'a> {
    let mut keys: Row<'a, Message> = row![].spacing(4).align_y(Alignment::Center);
    for (j, k) in c.chips().into_iter().enumerate() {
        if j > 0 {
            keys = keys.push(text("+").size(11).color(FAINT));
        }
        keys = keys.push(key_chip(k));
    }
    keys.into()
}

fn general<'a>(v: &View<'a>) -> El<'a> {
    let mut pages: Row<'a, Message> = row![].spacing(6);
    for (id, label) in START_PAGES {
        if crate::modules::ALL.iter().any(|m| m.page_id() == id && !v.modules.shown(*m)) {
            continue; // a hidden section can't be the start page
        }
        let active = v.store.start_page == id;
        pages = pages.push(button(text(label).size(12)).padding([5, 10]).style(chip(active)).on_press(msg(AppSettingsMsg::StartPage(id))));
    }
    column![
        section(
            "STARTUP",
            column![
                setting("Open on", "The page shown after signing in.", space().into()),
                container(pages).padding(Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 14.0 }),
            ]
            .spacing(8)
            .into(),
        ),
        section(
            "SERVER",
            column![
                setting("Signed in to", "To use another server, log out and change it on the sign-in screen.", text(v.server.unwrap_or("not signed in")).size(12.5).font(Font::MONOSPACE).color(TEXT_BODY).into()),
            ]
            .into(),
        ),
    ]
    .spacing(22)
    .into()
}

fn appearance<'a>(v: &View<'a>) -> El<'a> {
    let mut scales: Row<'a, Message> = row![].spacing(6);
    for s in SCALES {
        let active = (v.store.ui_scale - s).abs() < 0.01;
        scales = scales.push(button(text(format!("{}%", (s * 100.0).round())).size(12).font(if active { BOLD } else { INTER })).padding([5, 10]).style(chip(active)).on_press(msg(AppSettingsMsg::Scale(s))));
    }
    column![
        section(
            "INTERFACE",
            column![
                setting("Scale", "Size of everything in the window. Also Ctrl + / Ctrl − / Ctrl 0.", space().into()),
                container(scales).padding(Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 14.0 }),
            ]
            .spacing(8)
            .into(),
        ),
        section(
            "CHAT",
            setting("Play GIFs and stickers", "Off: they stand still on their first frame — calmer, and lighter on the battery.", switch_for(v.store.autoplay_gifs, Pref::AutoplayGifs)),
        ),
    ]
    .spacing(22)
    .into()
}

fn privacy<'a>(v: &View<'a>) -> El<'a> {
    let s = v.store;
    column![
        section(
            "CHATS",
            column![
                setting("Blur messages", "Messages are covered until you point at them — for a shared screen, a call or a café.", switch_for(s.blur_messages, Pref::BlurMessages)),
                setting("Hide chat previews", "The last message under each chat in the list isn't shown.", switch_for(s.blur_previews, Pref::BlurPreviews)),
            ]
            .spacing(8)
            .into(),
        ),
        section(
            "WINDOW",
            column![
                setting("Hide when you switch away", "Covers the window as soon as another one is in front, so nothing shows when you share your screen or step away.", switch_for(s.hide_on_blur, Pref::HideOnBlur)),
                setting("Lock wallets when hidden", "Hiding the window also locks your wallets: the master password is needed again.", switch_for(s.lock_wallets_on_hide, Pref::LockWalletsOnHide)),
                row![
                    button(row![icon(icons::eye_off(), 13, Color::WHITE), text("Hide now").size(12.5).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center))
                        .padding([7, 12])
                        .style(tone_button(Tone::Accent))
                        .on_press(msg(AppSettingsMsg::HideNow)),
                ]
                .push(hotkeys::binding_of(&s.hotkeys, hotkeys::Action::HideScreen).map(|c| combo_chips(&c)))
                .spacing(6)
                .align_y(Alignment::Center),
            ]
            .spacing(8)
            .into(),
        ),
        text("Notification previews are in Settings → Notifications (per device).").size(11.5).color(FAINT),
    ]
    .spacing(22)
    .into()
}

fn shortcuts<'a>(v: &View<'a>) -> El<'a> {
    let over = &v.store.hotkeys;
    let mut top: Column<'a, Message> = column![setting(
        "Keyboard shortcuts",
        "Click a shortcut, then press the new keys — Esc to keep the old ones. Kept on this computer.",
        switch_for(v.store.shortcuts, Pref::Shortcuts)
    )]
    .spacing(8);
    if hotkeys::defs().any(|d| hotkeys::changed(over, d)) {
        top = top.push(row![
            space().width(Fill),
            button(row![icon(icons::rotate_ccw(), 12, TEXT_BODY), text("Reset all to defaults").size(12)].spacing(6).align_y(Alignment::Center))
                .padding([6, 10])
                .style(tone_button(Tone::Dark))
                .on_press(msg(AppSettingsMsg::ResetKeys)),
        ]);
    }
    let table = |rows: Column<'a, Message>| -> El<'a> { container(rows).width(Fill).clip(true).style(boxed(Color::TRANSPARENT, BORDER, 10.0)).into() };
    let mut col: Column<'a, Message> = column![top].spacing(22);
    for g in hotkeys::GROUPS {
        let mut rows: Column<'a, Message> = column![].spacing(0);
        for (i, d) in g.items.iter().enumerate() {
            rows = rows.push(shortcut_row(v, d, i % 2 == 0));
        }
        col = col.push(section(g.title, table(rows)));
    }
    let mut rows: Column<'a, Message> = column![].spacing(0);
    for (i, (keys, label)) in hotkeys::ALWAYS.iter().enumerate() {
        let mut chips: Row<'a, Message> = row![].spacing(4).align_y(Alignment::Center);
        for k in keys.iter() {
            chips = chips.push(key_chip(*k));
        }
        rows = rows.push(shaded(container(row![text(*label).size(13).color(TEXT_SECONDARY).width(Fill), chips].spacing(12).align_y(Alignment::Center)).width(Fill).padding([9, 14]), i % 2 == 0));
    }
    col.push(section("Always on — even with shortcuts off", table(rows))).into()
}

/// Every other row of a shortcuts table a touch lighter.
fn shaded<'a>(line: iced::widget::Container<'a, Message>, on: bool) -> El<'a> {
    if on {
        line.style(|_| container::Style { background: Some(rgba(0xffffff, 0.02).into()), ..Default::default() }).into()
    } else {
        line.into()
    }
}

/// One action: its name, its keys (click to record new ones), back to the default, none.
fn shortcut_row<'a>(v: &View<'a>, d: &'static hotkeys::Def, shade: bool) -> El<'a> {
    let over = &v.store.hotkeys;
    let bound = hotkeys::binding(over, d);
    let recording = v.recording == Some(d.action);
    let keys: El<'a> = match (&bound, recording) {
        (_, true) => text("Press the keys…").size(12).font(SEMIBOLD).color(INDIGO_SOFT).into(),
        (Some(c), false) => combo_chips(c),
        (None, false) => text("Not set").size(12).color(FAINT).into(),
    };
    let bind = button(keys)
        .padding([4, 8])
        .style(move |_, status| button::Style {
            background: Some(
                if recording {
                    rgba(0x6366f1, 0.16)
                } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    rgba(0xffffff, 0.07)
                } else {
                    Color::TRANSPARENT
                }
                .into(),
            ),
            border: border(if recording { rgba(0x6366f1, 0.6) } else { Color::TRANSPARENT }, 1.0, 7.0),
            text_color: TEXT_BODY,
            ..Default::default()
        })
        .on_press(msg(AppSettingsMsg::RecordKey(d.action)));
    let tool = |handle: svg::Handle, tip: String, on: Option<AppSettingsMsg>| -> El<'a> {
        let Some(m) = on else { return space().width(26).into() };
        iced::widget::tooltip(
            button(iced::widget::center(icon(handle, 13, MUTED))).width(26).height(26).padding(0).style(ghost(6.0, rgba(0xffffff, 0.08))).on_press(msg(m)),
            container(text(tip).size(11)).padding([4, 8]).style(theme::popover),
            iced::widget::tooltip::Position::Top,
        )
        .into()
    };
    let reset_tip = if d.default.is_empty() { "Back to no shortcut".to_string() } else { format!("Back to {}", d.default) };
    let reset = tool(icons::rotate_ccw(), reset_tip, hotkeys::changed(over, d).then_some(AppSettingsMsg::ResetKey(d.action)));
    let clear = tool(icons::x(), "No shortcut".into(), bound.is_some().then_some(AppSettingsMsg::ClearKey(d.action)));
    let mut body: Column<'a, Message> = column![row![text(d.label).size(13).color(TEXT_BODY).width(Fill), bind, reset, clear].spacing(4).align_y(Alignment::Center)].spacing(4);
    if let Some((a, note)) = v.note {
        if *a == d.action {
            body = body.push(text(note.as_str()).size(11.5).color(AMBER_TEXT));
        }
    }
    shaded(container(body).width(Fill).padding([6, 12]), shade)
}

fn modules<'a>(v: &View<'a>) -> El<'a> {
    use crate::modules::{Module, Off, ALL};
    let glyph = |m: Module| match m {
        Module::Cheques => icons::wallet(),
        Module::Invoices => icons::receipt(),
        Module::Drops => icons::send(),
        Module::Crowdfund => icons::flame(),
    };
    let mut rows: Column<'a, Message> = column![].spacing(8);
    for m in ALL {
        let reason = v.modules.off_reason(m);
        let control: El = match reason {
            Some(Off::Build) => text("not in this build").size(11.5).color(FAINT).into(),
            Some(Off::Server) => text("closed by the server").size(11.5).color(AMBER_TEXT).into(),
            _ => toggler(reason.is_none()).size(20).style(switch).on_toggle(move |on| msg(AppSettingsMsg::Module(m, on))).into(),
        };
        let on = reason.is_none();
        rows = rows.push(
            container(
                row![
                    container(icon(glyph(m), 16, if on { Color::WHITE } else { MUTED })).width(34).height(34).center(34).style(boxed(if on { rgba(0x6366f1, 0.22) } else { rgba(0xffffff, 0.04) }, Color::TRANSPARENT, 9.0)),
                    column![text(m.label()).size(13.5).font(SEMIBOLD).color(if on { TEXT_BODY } else { TEXT_SECONDARY }), text(m.hint()).size(11.5).color(TEXT_SECONDARY).line_height(1.4)].spacing(3).width(Fill),
                    control,
                ]
                .spacing(14)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding([11, 14])
            .style(theme::soft_row),
        );
    }
    column![
        section("SECTIONS", rows.into()),
        text("A section you turn off leaves the side rail, the chat's + menu and the attach window. Cheques, invoices, drops and campaigns people send you still show in the chat, and you can still claim and pay them.")
            .size(11.5)
            .color(FAINT)
            .line_height(1.45),
    ]
    .spacing(14)
    .into()
}

fn developer<'a>(v: &View<'a>) -> El<'a> {
    let forced = crate::devtools::forced_on();
    let on = forced || v.store.dev_mode;
    let switch: El = if forced {
        toggler(true).size(20).style(switch).into() // no on_toggle: ACCORDO_DEVTOOLS holds it on
    } else {
        toggler(v.store.dev_mode).size(20).style(switch).on_toggle(|on| msg(AppSettingsMsg::DevMode(on))).into()
    };
    let hint = if forced {
        "On because ACCORDO_DEVTOOLS is set for this run."
    } else {
        "The developer tools (F12 or Ctrl+Shift+I): network, socket and log of this app. Remembered on this computer."
    };
    let account: El = match v.developer {
        Some(true) => text("Developer").size(12.5).font(SEMIBOLD).color(GREEN).into(),
        Some(false) => text("Not a developer").size(12.5).color(TEXT_SECONDARY).into(),
        None => text("not signed in").size(12.5).color(FAINT).into(),
    };
    let mut col: Column<'a, Message> = column![
        section(
            "DEVELOPER MODE",
            column![
                setting("Developer mode", hint, switch),
                setting("Your account", "Set by Accordo's admins. Later, Developer mode will need it.", account),
            ]
            .spacing(8)
            .into(),
        ),
    ]
    .spacing(22);
    col = col.push(section(
        "TOOLS",
        column![
            setting(
                "Developer tools open",
                "Docked under the app or in their own window (the button in their corner). Open when the app closes, they open again next time.",
                row![
                    key_chip("F12"),
                    key_chip("Ctrl+Shift+I"),
                    if on {
                        El::from(toggler(v.devtools_open).size(20).style(crate::theme::switch).on_toggle(|open| msg(AppSettingsMsg::Devtools(open))))
                    } else {
                        toggler(false).size(20).style(crate::theme::switch).into() // Developer mode is off
                    },
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
            ),
            text(if on {
                "Kept in memory only, the last 500 entries. Sign-in headers, passwords, codes and keys are never recorded."
            } else {
                "Turn on Developer mode to use them. Turning it off forgets everything recorded."
            })
            .size(11.5)
            .color(FAINT)
            .line_height(1.45),
        ]
        .spacing(10)
        .into(),
    ));
    col.into()
}

fn storage<'a>(v: &View<'a>) -> El<'a> {
    let data_dir = crate::api::client::config_dir().map(|p| p.display().to_string()).unwrap_or_else(|| "—".into());
    let kept = match v.wallet_kept_until.and_then(chrono::DateTime::from_timestamp_millis) {
        Some(t) => format!("Your unlocked wallet keys are kept in the system's secret store until {} — a restart before then doesn't lock them.", t.with_timezone(&chrono::Local).format("%H:%M")),
        None => "With a lock timer, unlocked wallet keys are kept in the system's secret store until it runs out. “Until restart” keeps them in memory only.".into(),
    };
    column![
        setting(
            "App data",
            "Preferences, the encrypted wallet check and local cheques / invoices.",
            button(text("Open folder").size(12)).padding([5, 10]).style(tone_button(Tone::Dark)).on_press(msg(AppSettingsMsg::OpenDataFolder)).into(),
        ),
        text(data_dir).size(11.5).font(Font::MONOSPACE).color(TEXT_SECONDARY),
        setting(
            "Wallet keys",
            "Deletes the kept wallet unlock of every account on this computer and locks the wallets now. The next start asks for the master password.",
            button(text("Delete saved keys").size(12)).padding([5, 10]).style(tone_button(Tone::Danger)).on_press(msg(AppSettingsMsg::ForgetWalletKeys)).into(),
        ),
        text(kept).size(11.5).color(TEXT_SECONDARY).line_height(1.45),
        chat_key(v),
        text("The sign-in and the chat key are kept in the system's secret store too, not in this folder. The master password is never written anywhere.").size(11.5).color(FAINT).line_height(1.45),
    ]
    .spacing(10)
    .into()
}

/// Storage → Chat key: delete it here, and how long it stays. Both only with a backup on Accordo:
/// without one a deleted key can't come back, and the encrypted chats would be lost.
fn chat_key<'a>(v: &View<'a>) -> El<'a> {
    let Some(info) = v.chat_key else {
        return setting("Chat key", "Sign in to manage this computer's copy of your chat key.", space().into());
    };
    let lifetime = v.store.chat_key_lifetime;
    let delete = button(text("Delete chat key").size(12))
        .padding([5, 10])
        .style(tone_button(Tone::Danger))
        .on_press_maybe((info.has_backup && info.here).then(|| msg(AppSettingsMsg::ForgetChatKey)));
    let mut chips: Row<'_, Message> = row![].spacing(6);
    for (caption, l) in CHAT_KEY_LIFETIMES {
        chips = chips.push(button(text(caption).size(11)).padding([3, 8]).style(chip(lifetime == l)).on_press_maybe(info.has_backup.then(|| msg(AppSettingsMsg::ChatKeyLifetime(l)))));
    }
    let note = if !info.has_backup {
        "Your chat key has no backup on Accordo yet — set a chat password first (Settings → Chat encryption). Without it a deleted key can't come back and your encrypted chats would be lost, so until then this account's key stays here whatever is picked above."
    } else if !info.here {
        "Your chat key isn't on this computer now — restore it with your chat password (Settings → Chat encryption)."
    } else {
        match lifetime {
            Lifetime::Forever => "Kept here until you delete it.",
            Lifetime::ThisRun => "Deleted when the app closes — you enter your chat password at every start.",
            Lifetime::Days(_) => "Deleted that long after it came to this computer; then you restore it with your chat password.",
        }
    };
    column![
        setting(
            "Chat key",
            "Deletes your chat key from this computer. The encrypted chats stay on Accordo and open again once you restore the key with your chat password.",
            delete.into(),
        ),
        text("Keep on this computer").size(12).font(SEMIBOLD).color(TEXT_BODY),
        chips,
        text(note).size(11.5).color(TEXT_SECONDARY).line_height(1.45),
    ]
    .spacing(8)
    .into()
}

/// The last minute as thin bars, newest on the right; `top` is the full height.
fn spark<'a>(values: Vec<f32>, top: f32, color: Color) -> El<'a> {
    const H: f32 = 34.0;
    let mut bars: Row<'a, Message> = row![].spacing(1).align_y(Alignment::End).height(H);
    for _ in values.len()..usage::HISTORY {
        bars = bars.push(space().width(3).height(1));
    }
    for v in values {
        let h = if top > 0.0 { (v / top * H).clamp(1.0, H) } else { 1.0 };
        bars = bars.push(container(space()).width(3).height(h).style(move |_| container::Style {
            background: Some(color.into()),
            border: border(Color::TRANSPARENT, 0.0, 1.0),
            ..Default::default()
        }));
    }
    bars.into()
}

/// A big number with a line under it and the minute's chart.
fn tile<'a>(title: &'a str, value: String, hint: String, chart: El<'a>) -> El<'a> {
    container(
        column![
            text(title).size(10.5).font(BOLD).color(TEXT_SECONDARY),
            text(value).size(22).font(BOLD).color(Color::WHITE),
            text(hint).size(11.5).color(TEXT_SECONDARY),
            chart,
        ]
        .spacing(5),
    )
    .width(Fill)
    .padding([12, 14])
    .style(theme::soft_row)
    .into()
}

fn usage_section<'a>(u: &'a Usage) -> El<'a> {
    let Some(s) = u.now.as_ref() else {
        return text("Measuring…").size(12.5).color(TEXT_SECONDARY).into();
    };
    let share = if s.total_memory > 0 { s.memory as f64 / s.total_memory as f64 * 100.0 } else { 0.0 };
    let mem_top = u.memory_history.iter().copied().max().unwrap_or(0) as f32 * 1.15;
    let cpu_top = u.cpu_history.iter().copied().fold(5.0_f32, f32::max) * 1.15;
    let tiles = row![
        tile(
            "MEMORY",
            usage::bytes(s.memory),
            format!("peak {} · {share:.1}% of {}", usage::bytes(s.peak), usage::bytes(s.total_memory)),
            spark(u.memory_history.iter().map(|m| *m as f32).collect(), mem_top, rgba(0x818cf8, 0.85)),
        ),
        tile(
            "CPU",
            format!("{:.1}%", s.cpu),
            format!("of all {} cores · last second", s.cores),
            spark(u.cpu_history.iter().copied().collect(), cpu_top, rgba(0x34d399, 0.85)),
        ),
    ]
    .spacing(10);
    let line = |k: &'a str, val: String| -> El<'a> { row![text(k).size(12.5).color(TEXT_SECONDARY).width(170), text(val).size(12.5).color(TEXT_BODY)].into() };
    let (pics, frames, pic_bytes) = s.pictures;
    let mut lines: Column<'a, Message> = column![line("Running for", usage::duration(s.uptime_s))].spacing(8);
    if let Some(t) = s.threads {
        lines = lines.push(line("Threads", t.to_string()));
    }
    lines = lines
        .push(line("Pictures in memory", if pics == 0 { "none".into() } else { format!("{pics} ({frames} frames) · {}", usage::bytes(pic_bytes)) }))
        .push(line("Disk since start", format!("{} read · {} written", usage::bytes(s.read), usage::bytes(s.written))))
        .push(line("App data on disk", s.data_bytes.map(usage::bytes).unwrap_or_else(|| "—".into())));
    column![
        tiles,
        lines,
        text("Measured once a second while this tab is open, for this app only. Graphics memory on the GPU isn't counted.").size(11.5).color(FAINT).line_height(1.45),
    ]
    .spacing(14)
    .into()
}

fn about<'a>(v: &View<'a>) -> El<'a> {
    let backend = std::env::var("ICED_BACKEND").unwrap_or_else(|_| "GPU (wgpu)".into());
    let line = |k: &'a str, val: String| -> El<'a> { row![text(k).size(12.5).color(TEXT_SECONDARY).width(170), text(val).size(12.5).color(TEXT_BODY)].into() };
    column![
        row![svg(icons::logo()).width(40).height(40), column![text("Accordo Desktop").size(18).font(BOLD).color(Color::WHITE), text("Native client · GPL-3.0-or-later").size(12).color(TEXT_SECONDARY)].spacing(2)]
            .spacing(14)
            .align_y(Alignment::Center),
        column![
            line("Version", env!("CARGO_PKG_VERSION").to_string()),
            line("Build", if cfg!(debug_assertions) { "debug".into() } else { "release".into() }),
            line("System", format!("{} · {}", std::env::consts::OS, std::env::consts::ARCH)),
            line("Renderer", backend),
        ]
        .spacing(8),
        section("USAGE", usage_section(v.usage)),
        text("Third-party licences: THIRD_PARTY.md in the source.").size(11.5).color(FAINT),
    ]
    .spacing(20)
    .into()
}

pub fn view<'a>(v: View<'a>) -> El<'a> {
    let nav_item = |t: Tab, handle: iced::widget::svg::Handle, label: &'static str| -> El<'a> {
        let active = v.tab == t;
        button(row![icon(handle, 15, if active { Color::WHITE } else { MUTED }), text(label).size(13).font(SEMIBOLD)].spacing(10).align_y(Alignment::Center))
            .width(Fill)
            .padding([9, 12])
            .style(selectable(active, rgba(0x6366f1, 0.22), 8.0))
            .on_press(msg(AppSettingsMsg::Tab(t)))
            .into()
    };
    let side = container(
        column![
            column![text("App settings").size(16).font(BOLD).color(Color::WHITE), text("This computer only").size(11.5).color(TEXT_SECONDARY)].spacing(3),
            container(space().width(Fill).height(1)).style(rule),
            column![
                nav_item(Tab::General, icons::settings(), "General"),
                nav_item(Tab::Appearance, icons::palette(), "Appearance"),
                nav_item(Tab::Privacy, icons::eye_off(), "Privacy"),
                nav_item(Tab::Shortcuts, icons::keyboard(), "Shortcuts"),
                nav_item(Tab::Modules, icons::blocks(), "Modules"),
            ]
            .spacing(4),
            container(space().width(Fill).height(1)).style(rule),
            column![nav_item(Tab::Developer, icons::code(), "Developer"), nav_item(Tab::Storage, icons::hard_drive(), "Storage"), nav_item(Tab::About, icons::info(), "About")].spacing(4),
        ]
        .spacing(14),
    )
    .width(220)
    .height(Fill)
    .padding([22, 14])
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0));

    let (heading, sub) = match v.tab {
        Tab::General => ("General", "Startup and the server"),
        Tab::Appearance => ("Appearance", "Size and motion"),
        Tab::Privacy => ("Privacy", "What shows on your screen"),
        Tab::Shortcuts => ("Shortcuts", "Do things without the mouse"),
        Tab::Modules => ("Modules", "The sections you use"),
        Tab::Developer => ("Developer", "Tools for building on Accordo"),
        Tab::Storage => ("Storage", "What this app keeps on this computer"),
        Tab::About => ("About", "Version, build and what the app uses"),
    };
    let header = row![
        column![text(heading).size(18).font(BOLD).color(Color::WHITE), text(sub).size(12).color(TEXT_SECONDARY)].spacing(2),
        space().width(Fill),
        button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(AppSettingsMsg::Close)),
    ]
    .align_y(Alignment::Center);
    let body = match v.tab {
        Tab::General => general(&v),
        Tab::Appearance => appearance(&v),
        Tab::Privacy => privacy(&v),
        Tab::Shortcuts => shortcuts(&v),
        Tab::Modules => modules(&v),
        Tab::Developer => developer(&v),
        Tab::Storage => storage(&v),
        Tab::About => about(&v),
    };
    let main = column![
        container(header).padding(Padding { top: 20.0, right: 20.0, bottom: 14.0, left: 26.0 }),
        container(space().width(Fill).height(1)).style(rule),
        scrollable(container(body).width(Fill).padding(Padding { top: 18.0, right: 26.0, bottom: 24.0, left: 26.0 })).height(Fill).style(thin_scroll),
    ]
    .width(Fill);
    let window = container(row![side, main]).width(Length::Fixed(900.0)).height(Length::Fixed(620.0)).style(modal);
    overlay::modal(window, msg(AppSettingsMsg::Close))
}
