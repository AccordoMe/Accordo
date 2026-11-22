// SPDX-License-Identifier: GPL-3.0-or-later
//! The developer tools (F12), in a window of their own — beside the app, on another screen — laid
//! out like the app's other windows: the sections on the left —
//! Network, Socket, Console over devtools' buffer (a list, the selected entry's details beside
//! it) and API, the module API console (call the protocol modules use, as a test module). The last
//! section and the filter are remembered (AppStore); the entries themselves never leave memory.

use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, space, text, text_input, Column};
use iced::{Alignment, Color, Fill, Font, Padding};

use super::ui::{icon, El};
use crate::app::module_console::Console;
use crate::app::{Message, ModMsg};
use crate::devtools::{self, Entry, Kind};
use crate::icons;
use crate::theme::{self, *};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tab {
    #[default]
    Network,
    Socket,
    Console,
    Api,
}

#[derive(Debug, Clone)]
pub enum DevMsg {
    Toggle,
    /// Into a window of its own / back under the app.
    Detach,
    Dock,
    Tab(Tab),
    Select(u64),
    Filter(String),
    Clear,
    Copy(String),
    /// Redraw while open (new entries).
    Tick,
}

#[derive(Debug, Default)]
pub struct State {
    pub open: bool,
    /// Its own window, while open detached (main.rs runs the app as a daemon for this); None and
    /// open: docked under the app.
    pub window: Option<iced::window::Id>,
    /// Its window is closing to dock it again (not to close the tools).
    pub docking: bool,
    pub tab: Tab,
    pub selected: Option<u64>,
    pub filter: String,
    /// devtools::version() at the last redraw.
    pub seen: u64,
}

const MONO: Font = Font::MONOSPACE;

fn msg(m: DevMsg) -> Message {
    Message::Dev(m)
}

fn matches(e: &Entry, tab: Tab, filter: &str) -> bool {
    let f = filter.to_lowercase();
    match (&e.kind, tab) {
        (Kind::Net(n), Tab::Network) => f.is_empty() || n.path.to_lowercase().contains(&f) || n.method.to_lowercase().contains(&f) || n.status.is_some_and(|s| s.to_string().starts_with(&f)),
        (Kind::Socket(s), Tab::Socket) => f.is_empty() || s.label.to_lowercase().contains(&f) || s.body.as_deref().is_some_and(|b| b.to_lowercase().contains(&f)),
        (Kind::Log(l), Tab::Console) => f.is_empty() || l.message.to_lowercase().contains(&f) || l.level.as_str().to_lowercase() == f,
        _ => false,
    }
}

fn status_color(status: Option<u16>) -> Color {
    match status {
        Some(200..=299) => GREEN,
        Some(300..=399) => SKY,
        Some(400..=499) => AMBER_TEXT,
        _ => DANGER_TEXT,
    }
}

fn level_color(level: tracing::Level) -> Color {
    match level {
        tracing::Level::ERROR => DANGER_TEXT,
        tracing::Level::WARN => AMBER_TEXT,
        tracing::Level::INFO => SKY,
        _ => MUTED,
    }
}

fn size(bytes: usize) -> String {
    if bytes < 1024 { format!("{bytes} B") } else if bytes < 1024 * 1024 { format!("{:.1} kB", bytes as f64 / 1024.0) } else { format!("{:.1} MB", bytes as f64 / 1048576.0) }
}

/// The module API console (app/module_console.rs): the protocol's methods on the left, the call
/// and its log in the middle, the sandbox module on the right.
fn api_view<'a>(filter: &str, c: &'a Console) -> El<'a> {
    use crate::module_host::{permission_text, spec};
    let cmsg = |m: ModMsg| Message::Module(m);
    let f = filter.to_lowercase();
    let api = spec::get();
    // The methods by group, as assets/module-api.json lists them.
    let mut methods = Column::new().spacing(2);
    let mut group = "";
    for m in &api.methods {
        if !f.is_empty() && !m.method.to_lowercase().contains(&f) && !m.summary.to_lowercase().contains(&f) {
            continue;
        }
        if m.group != group {
            group = &m.group;
            methods = methods.push(container(text(m.group.to_uppercase()).size(10).font(BOLD).color(FAINT)).padding(iced::Padding { top: 8.0, bottom: 2.0, left: 8.0, right: 0.0 }));
        }
        let on = c.method == m.method;
        let mut name = row![text(m.method.clone()).size(12).font(MONO).color(if on { Color::WHITE } else { TEXT_BODY }).width(Fill)].spacing(4).align_y(Alignment::Center);
        if m.window {
            name = name.push(icon(icons::app_window(), 11, MUTED));
        }
        if m.permission.is_some() {
            name = name.push(icon(icons::lock(), 11, AMBER_TEXT));
        }
        methods = methods.push(
            button(column![name, text(m.summary.clone()).size(10.5).color(FAINT)].spacing(1))
                .width(Fill)
                .padding([5, 8])
                .style(selectable(on, rgba(0x6366f1, 0.2), 6.0))
                .on_press(cmsg(ModMsg::Pick(m.method.clone()))),
        );
    }
    let left = container(scrollable(methods).height(Fill).style(thin_scroll)).width(270).padding([4, 8]);

    let ms = spec::find(&c.method);
    let chip = |t: String, color: Color| -> El<'a> { container(text(t).size(10.5).color(color)).padding([2, 7]).style(boxed(rgba(0xffffff, 0.04), rgba(0xffffff, 0.08), 6.0)).into() };
    let mut chips = row![].spacing(6);
    if let Some(m) = ms {
        if let Some(p) = &m.permission {
            chips = chips.push(chip(format!("needs {p}"), AMBER_TEXT));
        }
        if m.window {
            chips = chips.push(chip("may open a window".into(), INDIGO_SOFT));
        }
    }
    let tab = |label: &'static str, on: bool, raw: bool| -> El<'a> {
        button(text(label).size(11.5).font(SEMIBOLD).color(if on { Color::WHITE } else { MUTED })).padding([4, 10]).style(selectable(on, rgba(0x6366f1, 0.2), 6.0)).on_press(cmsg(ModMsg::Raw(raw))).into()
    };
    let params: El = if let (false, Some(m)) = (c.raw, ms) {
        let mut t = Column::new().spacing(6);
        let list = &m.params;
        for p in list {
            let name = p.name.clone();
            t = t.push(
                column![
                    row![
                        row![text(p.name.clone()).size(12).font(MONO).color(TEXT_BODY), text(if p.required { "*" } else { "" }).size(12).color(DANGER_TEXT)].spacing(2).width(130),
                        text_input(if p.required { "required" } else { "optional" }, c.fields.get(&p.name).map(String::as_str).unwrap_or("")).size(12).font(MONO).padding([5, 8]).style(field).on_input(move |v| Message::Module(ModMsg::Field(name.clone(), v))).on_submit(cmsg(ModMsg::Call)),
                        container(text(p.kind.clone()).size(10.5).font(MONO).color(INDIGO_SOFT)).width(64).align_x(iced::alignment::Horizontal::Right),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    container(text(p.description.clone()).size(10.5).color(FAINT)).padding(iced::Padding { left: 138.0, ..Default::default() }),
                ]
                .spacing(2),
            );
        }
        if list.is_empty() {
            t = t.push(text("No params").size(11.5).color(FAINT));
        }
        t.into()
    } else {
        iced::widget::text_editor(&c.params).on_action(|a| Message::Module(ModMsg::Params(a))).font(MONO).size(12).height(130).padding(8).into()
    };
    let docs: El = match ms {
        Some(m) => {
            let returns = serde_json::to_string(&m.returns).unwrap_or_default();
            let errors = m.errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" · ");
            column![
                row![text("returns").size(10.5).font(BOLD).color(FAINT).width(56), text(super::ui::truncate(&returns, 220)).size(11).font(MONO).color(TEXT_SECONDARY)].spacing(8),
                row![text("errors").size(10.5).font(BOLD).color(FAINT).width(56), text(if errors.is_empty() { "—".to_string() } else { errors }).size(11).font(MONO).color(TEXT_SECONDARY)].spacing(8),
            ]
            .spacing(4)
            .into()
        }
        None => space().into(),
    };

    let call = column![
        row![
            text(c.method.clone()).size(14).font(MONO).color(Color::WHITE),
            space().width(Fill),
            button(row![icon(icons::send(), 12, Color::WHITE), text("Call").size(12.5).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center))
                .padding([6, 14])
                .style(tone_button(Tone::Accent))
                .on_press_maybe(c.host.is_some().then(|| cmsg(ModMsg::Call))),
        ]
        .align_y(Alignment::Center),
        chips,
        row![tab("Params", !c.raw, false), tab("Raw JSON", c.raw, true)].spacing(4),
        params,
        docs,
    ]
    .spacing(8);

    let mut log = Column::new().spacing(6);
    for e in c.log.iter().rev() {
        let (chip, color) = match &e.result {
            None => ("waiting".to_string(), AMBER_TEXT),
            Some(Ok(_)) if e.event => ("event".to_string(), INDIGO_SOFT),
            Some(Ok(_)) => ("ok".to_string(), GREEN),
            Some(Err((code, _))) => (code.to_string(), DANGER_TEXT),
        };
        let body = match &e.result {
            Some(Ok(v)) => v.clone(),
            Some(Err((code, m))) => format!("{{ \"code\": {code}, \"message\": \"{m}\" }}"),
            None => "Waiting for the user…".to_string(),
        };
        let mut head = row![
            text(format!("#{}", e.id)).size(11).font(MONO).color(FAINT),
            text(if e.event { format!("⚡ {}", e.method) } else { format!("→ {}", e.method) }).size(12).font(MONO).color(TEXT_BODY).width(Fill),
            container(text(chip).size(10.5).font(BOLD).color(color)).padding([1, 6]).style(boxed(rgba(0xffffff, 0.04), Color::TRANSPARENT, 5.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if let Some(ms) = e.millis {
            head = head.push(text(format!("{ms} ms")).size(10.5).font(MONO).color(MUTED));
        }
        head = head.push(button(icon(icons::copy(), 11, MUTED)).padding(3).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press(msg(DevMsg::Copy(body.clone()))));
        let mut item = column![head].spacing(4);
        if !e.params.is_empty() && e.params != "{}" {
            item = item.push(text(super::ui::truncate(&e.params, 300)).size(11).font(MONO).color(FAINT));
        }
        item = item.push(text(body).size(11.5).font(MONO).color(if matches!(e.result, Some(Err(_))) { DANGER_TEXT } else { TEXT_BODY }));
        if let Some(Err((code, _))) = &e.result {
            if let Some(what) = api.errors.get(&code.to_string()) {
                item = item.push(text(format!("{code}: {what}")).size(10.5).color(FAINT));
            }
        }
        log = log.push(container(item).width(Fill).padding([8, 10]).style(boxed(rgba(0xffffff, 0.02), rgba(0xffffff, 0.05), 8.0)));
    }
    if c.log.is_empty() {
        log = log.push(text("Start with accordo_handshake: it opens the consent screen in the app window and gives the session every other call needs.").size(12).color(FAINT).line_height(1.45));
    }
    let middle = column![
        call,
        row![text("LOG").size(10.5).font(BOLD).color(FAINT), space().width(Fill), button(text("Clear").size(11).color(MUTED)).padding([2, 6]).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press(cmsg(ModMsg::ClearLog))].align_y(Alignment::Center),
        scrollable(log).height(Fill).style(thin_scroll),
    ]
    .spacing(10)
    .padding([10, 14])
    .width(Fill);

    let right: El = match &c.host {
        None => container(text("Sign in to use the console").size(12).color(FAINT)).padding(14).into(),
        Some(h) => {
            let mut perms = Column::new().spacing(4);
            for p in crate::api::mini_apps::all_permissions() {
                let registered = h.registered.iter().any(|r| r == p);
                let state: (&str, Color) = match h.grants.get(p) {
                    Some(g) if g.granted => ("granted", GREEN),
                    Some(g) if g.decided => ("declined", DANGER_TEXT),
                    _ => ("—", FAINT),
                };
                let key = p.to_string();
                perms = perms.push(
                    row![
                        iced::widget::checkbox(registered).size(13).style(tick).on_toggle(move |v| cmsg(ModMsg::Register(key.clone(), v))),
                        iced::widget::tooltip(text(p).size(11.5).font(MONO).color(if registered { TEXT_BODY } else { FAINT }), container(text(permission_text(p)).size(11)).padding([4, 8]).style(boxed(hex(0x1b1c25), BORDER, 6.0)), iced::widget::tooltip::Position::Left),
                        space().width(Fill),
                        text(state.0).size(10.5).color(state.1),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }
            let session = match &h.session {
                Some(s) => text(format!("session {s}")).size(11).font(MONO).color(GREEN),
                None => text("No session — call accordo_handshake").size(11).color(AMBER_TEXT),
            };
            let mut col = column![
                text("TEST MODULE").size(10.5).font(BOLD).color(FAINT),
                column![text(h.name.clone()).size(13.5).font(BOLD).color(Color::WHITE), text(h.origin.clone()).size(11).font(MONO).color(TEXT_SECONDARY), session].spacing(2),
            ]
            .spacing(8);
            if let Some(t) = &h.title {
                col = col.push(text(format!("Panel title: {t}")).size(11).color(TEXT_SECONDARY));
            }
            if !c.waits.is_empty() {
                col = col.push(
                    row![
                        text(format!("{} waiting on the user", c.waits.len())).size(11.5).color(AMBER_TEXT).width(Fill),
                        button(text("Cancel").size(11).color(TEXT_BODY)).padding([2, 8]).style(tone_button(Tone::Dark)).on_press(cmsg(ModMsg::CancelFront)),
                    ]
                    .align_y(Alignment::Center),
                );
            }
            col = col
                .push(container(space().width(Fill).height(1)).style(rule))
                .push(row![text("REGISTRATION").size(10.5).font(BOLD).color(FAINT), space().width(Fill), button(text("Reset answers").size(11).color(MUTED)).padding([2, 6]).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press(cmsg(ModMsg::ResetGrants))].align_y(Alignment::Center))
                .push(perms)
                .push(text("Ticked: what its registration lists — it can ask for nothing else (4100). Answers live in memory; the windows open in the app window, as a module's user sees them.").size(10.5).color(FAINT).line_height(1.4));
            scrollable(col.padding([10, 14])).height(Fill).style(thin_scroll).into()
        }
    };
    row![
        left,
        container(space().width(1).height(Fill)).style(theme::rule),
        middle,
        container(space().width(1).height(Fill)).style(theme::rule),
        container(right).width(290).height(Fill),
    ]
    .into()
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Network, Tab::Socket, Tab::Console, Tab::Api];

    /// The tabs on the list. The module API console is hidden until modules ship; ACCORDO_MODULE_API=1
    /// shows it (development).
    pub fn shown() -> Vec<Tab> {
        let api = std::env::var("ACCORDO_MODULE_API").is_ok_and(|v| v == "1");
        Tab::ALL.into_iter().filter(|t| *t != Tab::Api || api).collect()
    }

    fn label(self) -> &'static str {
        match self {
            Tab::Network => "Network",
            Tab::Socket => "Socket",
            Tab::Console => "Console",
            Tab::Api => "API",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Tab::Network => "Every request to the Accordo API",
            Tab::Socket => "The live connection: what came in, what went out",
            Tab::Console => "This app's log",
            Tab::Api => "The module API: call it as a test module would",
        }
    }

    fn glyph(self) -> iced::widget::svg::Handle {
        match self {
            Tab::Network => icons::activity(),
            Tab::Socket => icons::webhook(),
            Tab::Console => icons::code(),
            Tab::Api => icons::cpu(),
        }
    }
}

fn entry_line<'a>(e: &Entry) -> El<'a> {
    let time = text(e.at.format("%H:%M:%S%.3f").to_string()).size(11).font(MONO).color(FAINT);
    match &e.kind {
        Kind::Net(n) => row![
            time,
            text(n.method.clone()).size(11.5).font(MONO).color(INDIGO_SOFT).width(56),
            text(n.status.map_or("ERR".to_string(), |s| s.to_string())).size(11.5).font(MONO).color(status_color(n.status)).width(36),
            container(text(n.path.clone()).size(11.5).font(MONO).color(TEXT_BODY).wrapping(Wrapping::None)).width(Fill).clip(true),
            text(format!("{} ms", n.millis)).size(11).font(MONO).color(MUTED).width(64),
            text(size(n.size)).size(11).font(MONO).color(FAINT).width(64),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into(),
        Kind::Socket(s) => row![
            time,
            text(if s.incoming { "↓" } else { "•" }).size(12).font(MONO).color(if s.incoming { GREEN } else { MUTED }).width(14),
            container(text(s.label.clone()).size(11.5).font(MONO).color(TEXT_BODY).wrapping(Wrapping::None)).width(Fill).clip(true),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into(),
        Kind::Log(l) => row![
            time,
            text(l.level.as_str()).size(11).font(MONO).color(level_color(l.level)).width(44),
            container(text(l.message.clone()).size(11.5).font(MONO).color(TEXT_BODY).wrapping(Wrapping::None)).width(Fill).clip(true),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into(),
    }
}

pub fn view<'a>(state: &'a State, console: &'a Console) -> El<'a> {
    let all = devtools::snapshot();
    let entries: Vec<&Entry> = all.iter().filter(|e| matches(e, state.tab, &state.filter)).collect();
    let count = |t: Tab| -> usize { if t == Tab::Api { console.log.iter().filter(|e| !e.event).count() } else { all.iter().filter(|e| matches(e, t, "")).count() } };

    let nav_item = |t: Tab| -> El<'_> {
        let active = state.tab == t;
        let n = count(t);
        button(
            row![
                icon(t.glyph(), 15, if active { Color::WHITE } else { MUTED }),
                text(t.label()).size(13).font(SEMIBOLD).width(Fill),
                text(if n > 0 { n.to_string() } else { String::new() }).size(11).font(MONO).color(if active { TEXT_BODY } else { FAINT }),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .width(Fill)
        .padding([9, 12])
        .style(selectable(active, rgba(0x6366f1, 0.22), 8.0))
        .on_press(msg(DevMsg::Tab(t)))
        .into()
    };
    let side = container(
        column![
            column![text("Developer tools").size(16).font(BOLD).color(Color::WHITE), text("This app · in memory only").size(11.5).color(TEXT_SECONDARY)].spacing(3),
            container(space().width(Fill).height(1)).style(rule),
            Column::with_children(Tab::shown().into_iter().map(nav_item)).spacing(4),
            space().height(Fill),
            button(row![icon(icons::trash(), 13, TEXT_BODY), text("Clear everything").size(12.5).font(SEMIBOLD)].spacing(7).align_y(Alignment::Center))
                .width(Fill)
                .padding([8, 12])
                .style(tone_button(Tone::Dark))
                .on_press(msg(DevMsg::Clear)),
            text("The last 500 entries. Sign-in headers, passwords, codes and keys are never recorded.").size(11).color(FAINT).line_height(1.4),
        ]
        .spacing(14),
    )
    .width(230)
    .height(Fill)
    .padding([22, 14])
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0));

    let shown = if state.tab == Tab::Api { String::new() } else { format!("{} shown", entries.len()) };
    let header = row![
        column![text(state.tab.label()).size(18).font(BOLD).color(Color::WHITE), text(state.tab.hint()).size(12).color(TEXT_SECONDARY)].spacing(2),
        space().width(Fill),
        text(shown).size(11.5).color(FAINT),
        text_input("Filter…", &state.filter).size(12.5).padding([7, 10]).width(260).style(field).on_input(|v| msg(DevMsg::Filter(v))),
        {
            let docked = state.window.is_none();
            iced::widget::tooltip(
                button(icon(if docked { icons::external_link() } else { icons::arrow_down_left() }, 16, MUTED)).padding(7).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(if docked { DevMsg::Detach } else { DevMsg::Dock })),
                container(text(if docked { "Open in its own window" } else { "Dock under the app" }).size(11.5).color(TEXT_BODY)).padding([4, 8]).style(boxed(hex(0x1c1d22), BORDER, 6.0)),
                iced::widget::tooltip::Position::Bottom,
            )
        },
        button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(DevMsg::Toggle)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let body: El = if state.tab == Tab::Api {
        api_view(&state.filter, console)
    } else {
        let mut list = Column::new().spacing(1);
        for e in entries.iter().rev() {
            let selected = state.selected == Some(e.id);
            list = list.push(button(entry_line(e)).width(Fill).padding([3, 8]).style(selectable(selected, rgba(0x6366f1, 0.2), 4.0)).on_press(msg(DevMsg::Select(e.id))));
        }
        if entries.is_empty() {
            list = list.push(container(text(if state.filter.is_empty() { "Nothing yet" } else { "Nothing matches the filter" }).size(12).color(FAINT)).padding(12));
        }
        let details: El = match entries.iter().find(|e| Some(e.id) == state.selected) {
            Some(e) => details(e),
            None => container(text("Select an entry to see its details").size(12).color(FAINT)).padding(16).into(),
        };
        row![
            scrollable(list).width(iced::FillPortion(3)).height(Fill).style(thin_scroll),
            container(space().width(1).height(Fill)).style(theme::rule),
            container(details).width(iced::FillPortion(2)).height(Fill),
        ]
        .into()
    };

    let main = column![
        container(header).padding(Padding { top: 20.0, right: 20.0, bottom: 14.0, left: 26.0 }),
        container(space().width(Fill).height(1)).style(rule),
        container(body).width(Fill).height(Fill).padding(Padding { top: 6.0, right: 8.0, bottom: 8.0, left: 8.0 }),
    ]
    .width(Fill);
    container(row![side, main].padding(8).spacing(8)).width(Fill).height(Fill).style(|_| container::Style { background: Some(BG_DARK.into()), ..Default::default() }).into()
}

fn block<'a>(title: &'static str, content: String) -> El<'a> {
    column![
        row![
            text(title).size(11).font(BOLD).color(LABEL),
            space().width(Fill),
            button(text("Copy").size(10.5)).padding([2, 8]).style(tone_button(Tone::Dark)).on_press(msg(DevMsg::Copy(content.clone()))),
        ]
        .align_y(Alignment::Center),
        container(text(content).size(11.5).font(MONO).color(TEXT_BODY)).width(Fill).padding(8).style(boxed(hex(0x101118), BORDER, 6.0)),
    ]
    .spacing(6)
    .into()
}

fn details<'a>(e: &Entry) -> El<'a> {
    let mut col = Column::new().spacing(12).padding(12);
    match &e.kind {
        Kind::Net(n) => {
            col = col.push(
                column![
                    text(format!("{} {}", n.method, n.url)).size(12).font(MONO).color(Color::WHITE),
                    text(format!(
                        "Status {} · {} ms · {} · {}",
                        n.status.map_or("— (not reached)".to_string(), |s| s.to_string()),
                        n.millis,
                        size(n.size),
                        e.at.format("%H:%M:%S")
                    ))
                    .size(11.5)
                    .color(status_color(n.status)),
                ]
                .spacing(4),
            );
            if let Some(err) = &n.error {
                col = col.push(text(err.clone()).size(12).color(DANGER_TEXT));
            }
            col = col.push(text("Authorization header and secrets in bodies are never recorded.").size(10.5).color(FAINT));
            if let Some(b) = &n.request_body {
                col = col.push(block("Request body", b.clone()));
            }
            col = col.push(block("Response", n.response_body.clone().unwrap_or_else(|| "(empty)".into())));
        }
        Kind::Socket(s) => {
            col = col.push(text(format!("{} {}", if s.incoming { "↓" } else { "•" }, s.label)).size(12).font(MONO).color(Color::WHITE));
            if let Some(b) = &s.body {
                col = col.push(block("Payload", b.clone()));
            }
        }
        Kind::Log(l) => {
            col = col.push(text(format!("{} · {}", l.level, l.target)).size(11.5).color(level_color(l.level)));
            col = col.push(block("Message", l.message.clone()));
        }
    }
    scrollable(col).height(Fill).style(thin_scroll).into()
}

