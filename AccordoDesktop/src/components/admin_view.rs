// SPDX-License-Identifier: GPL-3.0-or-later
//! The Admin Console window (pages/admin_page.rs holds its state): the sections on the left, the
//! open one on the right; a user's card opens as its own window over it.

use iced::widget::{button, checkbox, column, container, row, scrollable, space, text, text_input, toggler, Column, Row};
use iced::{Alignment, Color, Fill, Font, Length, Padding};
use serde_json::Value;

use crate::api::admin::{AdminUser, ReportFilter};
use crate::api::mini_apps::AppStatus;
use crate::app::Message;
use crate::components::overlay;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::pages::admin_page::{AdminMsg, AdminTab, Control, Profile, ReportKind, State, PLANS};
use crate::theme::*;

fn m(x: AdminMsg) -> Message {
    Message::Admin(x)
}

fn when(iso: &str) -> String {
    crate::components::chat::display::local(iso).map(|t| t.format("%b %-d, %Y").to_string()).unwrap_or_else(|| "—".into())
}

fn when_time(iso: &str) -> String {
    crate::components::chat::display::local(iso).map(|t| t.format("%b %-d, %Y %H:%M").to_string()).unwrap_or_else(|| "—".into())
}

fn card<'a>(content: impl Into<El<'a>>) -> El<'a> {
    container(content).width(Fill).padding([18, 20]).style(boxed(hex(0x11131d), BORDER, 12.0)).into()
}

fn caption<'a>(t: &'a str) -> El<'a> {
    text(t.to_uppercase()).size(10.5).font(BOLD).color(TEXT_SECONDARY).into()
}

fn small_btn<'a>(label: impl Into<String>, tone: Tone, on: Option<Message>) -> El<'a> {
    button(text(label.into()).size(12).font(SEMIBOLD)).padding([5, 12]).style(tone_button(tone)).on_press_maybe(on).into()
}

fn chip_btn<'a>(label: String, on: bool, msg: Message) -> El<'a> {
    button(text(label).size(11.5).font(if on { BOLD } else { INTER })).padding([4, 10]).style(chip(on)).on_press(msg).into()
}

fn badge<'a>(t: impl Into<String>, c: Color) -> El<'a> {
    container(text(t.into()).size(10).font(BOLD).color(c)).padding([2, 7]).style(boxed(Color { a: 0.15, ..c }, Color::TRANSPARENT, 5.0)).into()
}

fn loading<'a, T>(v: &'a Option<Result<T, String>>, what: &'a str) -> Result<&'a T, El<'a>> {
    match v {
        None => Err(container(text(format!("Loading {what}…")).size(13).color(MUTED)).center_x(Fill).padding(40).into()),
        Some(Err(e)) => Err(container(column![text(format!("Couldn't load {what}")).size(14).font(BOLD).color(DANGER_TEXT), text(e.clone()).size(12.5).color(TEXT_SECONDARY)].spacing(6).align_x(Alignment::Center)).center_x(Fill).padding(40).into()),
        Some(Ok(x)) => Ok(x),
    }
}

/// "+N" in a ring (the web's CircularProgress).
fn ring<'a>(value: usize, label: &'a str, color: Color) -> El<'a> {
    column![
        container(text(if value > 0 { format!("+{value}") } else { "0".into() }).size(13).font(MEDIUM).color(Color::WHITE)).width(64).height(64).center(64).style(move |_| container::Style {
            border: border(if value > 0 { color } else { rgba(0xffffff, 0.08) }, 3.5, 32.0),
            ..Default::default()
        }),
        text(label.to_uppercase()).size(11).font(MEDIUM).color(TEXT_SECONDARY),
    ]
    .spacing(10)
    .align_x(Alignment::Center)
    .into()
}

fn stat<'a>(label: &'a str, big: String, sub: String, dot: bool) -> El<'a> {
    let mut v = row![].spacing(8).align_y(Alignment::Center);
    if dot {
        v = v.push(container(space()).width(9).height(9).style(boxed(hex(0x57f287), Color::TRANSPARENT, 5.0)));
    }
    v = v.push(text(big).size(26).font(BOLD).color(Color::WHITE)).push(text(sub).size(12).color(rgba(0xffffff, 0.4)));
    container(column![caption(label), v].spacing(8)).width(Fill).padding([18, 20]).style(boxed(hex(0x11131d), BORDER, 12.0)).into()
}

fn ts(v: &Value) -> i64 {
    v.get("created_at").and_then(Value::as_str).and_then(crate::components::chat::display::local).map(|t| t.timestamp()).unwrap_or(0)
}

fn overview<'a>(st: &'a State) -> El<'a> {
    let users: &[AdminUser] = st.users.as_ref().and_then(|r| r.as_ref().ok()).map(Vec::as_slice).unwrap_or(&[]);
    let invoices: &[Value] = st.invoices.as_ref().and_then(|r| r.as_ref().ok()).map(Vec::as_slice).unwrap_or(&[]);
    let cheques: &[Value] = st.cheques.as_ref().and_then(|r| r.as_ref().ok()).map(Vec::as_slice).unwrap_or(&[]);
    let now = chrono::Utc::now().timestamp();
    let since = |d: i64| now - d * 86_400;
    let joined = |d: i64| users.iter().filter(|u| crate::components::chat::display::local(&u.created_at).is_some_and(|t| t.timestamp() > since(d))).count();
    let acts = |d: i64| cheques.iter().chain(invoices).filter(|v| ts(v) > since(d)).count();
    let amount = |v: &Value| v.get("amount_due").and_then(|a| a.as_f64().or_else(|| a.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
    let paid: f64 = invoices.iter().filter(|i| i.get("status").and_then(Value::as_str).is_some_and(|s| s.eq_ignore_ascii_case("paid"))).map(amount).sum();
    let unpaid: f64 = invoices.iter().filter(|i| !i.get("status").and_then(Value::as_str).is_some_and(|s| s.eq_ignore_ascii_case("paid"))).map(amount).sum();
    let suspended = users.iter().filter(|u| !u.is_active).count();
    let stats = row![
        stat("Total users", users.len().to_string(), format!("+ {} this wk", joined(7)), false),
        stat("Online now", st.online.len().to_string(), format!("{suspended} suspended"), true),
        stat("Total invoices", invoices.len().to_string(), "created".into(), false),
        stat("Total cheques", cheques.len().to_string(), "lifetime".into(), false),
        stat("Paid volume", format!("${}", crate::components::chat::cheque_card::amount(paid)), format!("+ ${} unpaid", crate::components::chat::cheque_card::amount(unpaid)), false),
    ]
    .spacing(14);
    let rings = |title: &'a str, vals: [usize; 3], c: Color| -> El<'a> {
        card(column![text(title).size(15).font(MEDIUM).color(Color::WHITE), row![ring(vals[0], "24 hours", c), space().width(Fill), ring(vals[1], "7 days", c), space().width(Fill), ring(vals[2], "30 days", c)].padding([0, 30])].spacing(22))
    };
    let mut col = column![stats, row![rings("User Growth", [joined(1), joined(7), joined(30)], hex(0x5865f2)), rings("System Actions", [acts(1), acts(7), acts(30)], hex(0x57f287))].spacing(14)].spacing(14);
    if let Some(Ok(r)) = &st.reg {
        let (open, c) = (r.enabled, if r.enabled { hex(0x57f287) } else { hex(0xed4245) });
        let limit = if r.max_users > 0 {
            format!("Early access limit: {} / {} accounts ({} spots remaining)", r.current_users, r.max_users, r.remaining_spots.unwrap_or(0))
        } else {
            format!("Unlimited early registrations ({} total users)", r.current_users)
        };
        let mut title = row![text("Registration Gatekeeper").size(15).font(SEMIBOLD).color(Color::WHITE), badge(if open { "OPEN" } else { "CLOSED" }, c)].spacing(8).align_y(Alignment::Center);
        if r.invite_code_required {
            title = title.push(badge("INVITE REQUIRED", AMBER_TEXT));
        }
        col = col.push(card(
            row![
                container(icon(if open { icons::lock_open() } else { icons::lock() }, 20, c)).width(42).height(42).center(42).style(boxed(Color { a: 0.15, ..c }, Color::TRANSPARENT, 10.0)),
                column![title, text(format!("{limit} • Anti-spam: max {} / IP", r.rate_limit_per_ip)).size(12).color(TEXT_SECONDARY)].spacing(4).width(Fill),
                small_btn(if open { "Close Registrations" } else { "Open Registrations" }, if open { Tone::Danger } else { Tone::Accent }, Some(m(AdminMsg::RegEnabled(!open)))),
                small_btn("Manage Controls →", Tone::Dark, Some(m(AdminMsg::Tab(AdminTab::Access)))),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        ));
    }
    col.into()
}

fn matches(q: &str, fields: &[&str]) -> bool {
    let q = q.trim().to_lowercase();
    q.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(&q))
}

fn header_row<'a>(cols: &[(&'a str, Length)]) -> El<'a> {
    let mut r: Row<'a, Message> = row![].spacing(10);
    for (t, w) in cols {
        r = r.push(text(*t).size(11.5).font(SEMIBOLD).color(TEXT_SECONDARY).width(*w));
    }
    container(r).padding([8, 12]).into()
}

fn users<'a>(st: &'a State) -> El<'a> {
    let list = match loading(&st.users, "users") {
        Ok(l) => l,
        Err(e) => return e,
    };
    let mut col: Column<'a, Message> = column![header_row(&[("ID", Length::Fixed(50.0)), ("Username", Length::FillPortion(3)), ("Discord", Length::FillPortion(2)), ("API login", Length::Fixed(80.0)), ("Inv / Chq", Length::Fixed(80.0)), ("Joined", Length::Fixed(110.0)), ("Status", Length::Fixed(90.0)), ("", Length::Fixed(100.0))])].spacing(2);
    for u in list.iter().filter(|u| matches(&st.search, &[&u.username, u.email.as_deref().unwrap_or(""), &u.id.to_string()])) {
        let online = st.online.contains(&u.id);
        let mut name = row![text(u.username.clone()).size(13).font(SEMIBOLD).color(Color::WHITE)].spacing(6).align_y(Alignment::Center);
        if online {
            name = name.push(container(space()).width(7).height(7).style(boxed(hex(0x57f287), Color::TRANSPARENT, 4.0)));
        }
        if u.is_admin {
            name = name.push(badge("ADMIN", INDIGO_SOFT));
        }
        let id = u.id;
        let row_ = row![
            text(format!("#{}", u.id)).size(12.5).color(TEXT_SECONDARY).width(50),
            column![name, text(u.email.clone().unwrap_or_else(|| "no email".into())).size(11.5).color(TEXT_SECONDARY)].spacing(2).width(Length::FillPortion(3)),
            text(u.discord_username.as_ref().map(|d| format!("@{d}")).unwrap_or_else(|| "-".into())).size(12.5).color(TEXT_BODY).width(Length::FillPortion(2)),
            container(toggler(u.api_login).size(16).on_toggle(move |v| m(AdminMsg::SetApiLogin(id, v)))).width(80),
            text(format!("{} / {}", u.invoice_count, u.cheque_count)).size(12.5).color(TEXT_BODY).width(80),
            text(when(&u.created_at)).size(12).color(TEXT_SECONDARY).width(110),
            container(badge(if u.is_active { "ACTIVE" } else { "SUSPENDED" }, if u.is_active { GREEN } else { DANGER_TEXT })).width(90),
            container(small_btn(if u.is_active { "Suspend" } else { "Activate" }, if u.is_active { Tone::Danger } else { Tone::Dark }, (!u.is_admin || !u.is_active).then(|| m(AdminMsg::SetActive(id, !u.is_active))))).width(100),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        col = col.push(
            button(row_)
                .width(Fill)
                .padding([8, 12])
                .style(move |_, s| button::Style { background: Some(rgba(0xffffff, if matches!(s, button::Status::Hovered) { 0.04 } else { 0.0 }).into()), text_color: TEXT, border: border(Color::TRANSPARENT, 0.0, 8.0), ..Default::default() })
                .on_press(m(AdminMsg::OpenUser(id))),
        );
        if !u.is_active {
            // (suspended rows read dimmer in the web; here the badge says it)
        }
    }
    col.into()
}

fn json_rows<'a>(st: &'a State, list: &'a [Value], owner_key: &'static str, id_key: &'static str, invoices: bool) -> El<'a> {
    let mut col: Column<'a, Message> = column![header_row(&[("ID", Length::Fixed(60.0)), ("Owner", Length::FillPortion(2)), ("Amount", Length::FillPortion(2)), ("Network", Length::FillPortion(2)), ("Status", Length::Fixed(110.0)), ("Created", Length::Fixed(120.0))])].spacing(2);
    for v in list.iter().filter(|v| matches(&st.search, &[v.get(owner_key).and_then(Value::as_str).unwrap_or(""), &v.get("id").map(|i| i.to_string()).unwrap_or_default(), v.get(id_key).and_then(Value::as_str).unwrap_or("")])) {
        let r = crate::components::finance::records::summary(v, invoices);
        let status = v.get("status").and_then(Value::as_str).unwrap_or("").to_string();
        let row_ = row![
            text(format!("#{}", v.get("id").map(|i| i.to_string()).unwrap_or_default())).size(12.5).color(TEXT_SECONDARY).width(60),
            text(v.get(owner_key).and_then(Value::as_str).unwrap_or("—").to_string()).size(12.5).font(SEMIBOLD).color(Color::WHITE).width(Length::FillPortion(2)),
            text(format!("{} {}", r.0, r.1)).size(12.5).font(Font::MONOSPACE).color(TEXT_BODY).width(Length::FillPortion(2)),
            text(r.2).size(12).color(TEXT_SECONDARY).width(Length::FillPortion(2)),
            container(text(status.to_uppercase()).size(10.5).font(BOLD).color(TEXT_BODY)).width(110),
            text(v.get("created_at").and_then(Value::as_str).map(when_time).unwrap_or_default()).size(12).color(TEXT_SECONDARY).width(120),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let picked = st.json.as_ref() == Some(v);
        col = col.push(
            button(row_)
                .width(Fill)
                .padding([8, 12])
                .style(move |_, s| button::Style { background: Some(if picked { rgba(0x6366f1, 0.12) } else { rgba(0xffffff, if matches!(s, button::Status::Hovered) { 0.04 } else { 0.0 }) }.into()), text_color: TEXT, border: border(Color::TRANSPARENT, 0.0, 8.0), ..Default::default() })
                .on_press(m(AdminMsg::ShowJson(Some(Box::new(v.clone()))))),
        );
    }
    col.into()
}

fn records_tab<'a>(st: &'a State, invoices: bool) -> El<'a> {
    let (data, what) = if invoices { (&st.invoices, "invoices") } else { (&st.cheques, "cheques") };
    let list = match loading(data, what) {
        Ok(l) => l,
        Err(e) => return e,
    };
    let table = if invoices { json_rows(st, list, "invoice_owner_identifier", "invoice_id", true) } else { json_rows(st, list, "cheque_owner_identificator", "cheque_id", false) };
    match &st.json {
        None => table,
        Some(v) => row![
            container(table).width(Length::FillPortion(3)),
            container(
                column![
                    row![text("Record").size(13).font(BOLD).color(Color::WHITE), space().width(Fill), button(icon(icons::copy(), 13, MUTED)).padding(5).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(Message::CopyText(serde_json::to_string_pretty(v).unwrap_or_default(), "Record")), button(icon(icons::x(), 14, MUTED)).padding(5).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(m(AdminMsg::ShowJson(None)))].spacing(4).align_y(Alignment::Center),
                    scrollable(text(serde_json::to_string_pretty(v).unwrap_or_default()).size(11.5).font(Font::MONOSPACE).color(TEXT_BODY)).height(520).style(thin_scroll),
                ]
                .spacing(8),
            )
            .width(Length::FillPortion(2))
            .padding(14)
            .style(boxed(rgba(0x000000, 0.25), BORDER, 10.0)),
        ]
        .spacing(12)
        .into(),
    }
}

fn field_input<'a>(label: &'a str, value: &str, width: f32, on: impl Fn(String) -> Message + 'a) -> El<'a> {
    column![text(label).size(11.5).color(TEXT_SECONDARY), text_input("", value).on_input(on).size(13).padding([7, 10]).width(width).style(field)].spacing(4).into()
}

/// The four limits in a user's Control: steppers — or, for an admin, what the default limits
/// file's Admin row gives (an admin's own values aren't applied; change them there).
fn admin_limits<'a>(u: &'a AdminUser, c: &'a Control) -> El<'a> {
    if u.is_admin {
        let v = |l: Option<i64>| l.map_or_else(|| "no limit".to_string(), |n| n.to_string());
        let shown = format!("groups {} · public rooms {} · Suggested {} · mini-apps {}", v(u.private_groups_limit), v(u.public_rooms_limit), v(u.suggested_rooms_limit), v(u.mini_apps_limit));
        return column![
            line(icons::shield(), "Limits", text(shown).size(11.5).color(TEXT_BODY).into()),
            text("An admin's limits come from Access & Limits → Default limits (the Admin row).").size(11).color(FAINT),
        ]
        .spacing(6)
        .into();
    }
    column![
        line(icons::globe(), "Public rooms", row![stepper(c.rooms_limit, 100, AdminMsg::RoomsLimit), text(format!("owns {}", u.public_rooms_owned)).size(11.5).color(TEXT_SECONDARY)].spacing(10).align_y(Alignment::Center).into()),
        line(icons::users(), "Private groups", row![num_input(&c.groups, 10_000, AdminMsg::Groups), text(format!("owns {}", u.private_groups_owned)).size(11.5).color(TEXT_SECONDARY)].spacing(10).align_y(Alignment::Center).into()),
        line(icons::app_window(), "Mini-apps", row![num_input(&c.apps, 1000, AdminMsg::AppsLimit), text(format!("owns {}", u.mini_apps_owned)).size(11.5).color(TEXT_SECONDARY)].spacing(10).align_y(Alignment::Center).into()),
        line(icons::sparkles(), "Suggested", stepper(c.suggested, 20, AdminMsg::Suggested)),
    ]
    .spacing(10)
    .into()
}

/// "Default limits": data/default_limits.json — what every account (by role) and room gets
/// without its own value.
fn defaults_card<'a>(st: &'a State) -> El<'a> {
    use crate::api::admin::{ACCOUNT_LIMITS, LIMIT_ROLES, ROOM_LIMITS};
    let body: El = match loading(&st.defaults, "default limits") {
        Err(e) => e,
        Ok(d) => {
            let file = d.get("file").and_then(serde_json::Value::as_bool).unwrap_or(false);
            let cell = |r: usize, i: usize| -> El<'a> {
                let hint = match r {
                    1 => "= user",
                    2 => "no limit",
                    _ => "",
                };
                text_input(hint, &st.def_form[r][i]).on_input(move |v| m(AdminMsg::DefField(r, i, v))).size(13).padding([7, 10]).width(110).style(field).into()
            };
            let mut head = row![text("").width(90)].spacing(12);
            for (_, label) in ACCOUNT_LIMITS {
                head = head.push(text(label).size(11.5).color(TEXT_SECONDARY).width(110));
            }
            let mut grid = column![head].spacing(8);
            for (r, (_, label)) in LIMIT_ROLES.iter().enumerate() {
                let mut line = row![text(*label).size(12.5).font(SEMIBOLD).color(TEXT_BODY).width(90)].spacing(12).align_y(Alignment::Center);
                for i in 0..ACCOUNT_LIMITS.len() {
                    line = line.push(cell(r, i));
                }
                if r == 0 {
                    line = line.push(row![toggler(st.def_developer).size(16).on_toggle(|v| m(AdminMsg::DefDeveloper(v))), text("Developer by default").size(11.5).color(TEXT_SECONDARY)].spacing(6).align_y(Alignment::Center));
                }
                grid = grid.push(line);
            }
            let mut rooms = row![].spacing(14);
            for (i, (_, label)) in ROOM_LIMITS.iter().enumerate() {
                rooms = rooms.push(field_input(label, &st.def_rooms[i], 170.0, move |v| m(AdminMsg::DefField(3, i, v))));
            }
            let source = if file { "Saved in data/default_limits.json on the server." } else { "No file yet: these are the server's .env values — saving creates data/default_limits.json." };
            column![
                text("ACCOUNTS — by role: admins take the Admin row, developers the Developer row, everyone else User").size(10.5).font(BOLD).color(FAINT),
                grid,
                text("Developer: empty = as User. Admin: empty = no limit. \"-\" = no limit.").size(11).color(FAINT),
                text("ROOMS").size(10.5).font(BOLD).color(FAINT),
                rooms,
                row![
                    text(format!("Applies at once to every account and room without its own value (set per user in Control, per room in Rooms). {source}")).size(11.5).color(TEXT_SECONDARY).width(Fill),
                    small_btn("Save defaults", Tone::Accent, Some(m(AdminMsg::DefSave))),
                ]
                .spacing(14)
                .align_y(Alignment::Center),
            ]
            .spacing(12)
            .into()
        }
    };
    card(column![text("Default limits").size(15).font(SEMIBOLD).color(Color::WHITE), body].spacing(14))
}

fn access<'a>(st: &'a State) -> El<'a> {
    let f = &st.reg_form;
    let reg = card(
        column![
            row![text("Registration").size(15).font(SEMIBOLD).color(Color::WHITE), space().width(Fill), text(if f.enabled { "Open" } else { "Closed" }).size(12.5).color(if f.enabled { GREEN } else { DANGER_TEXT }), toggler(f.enabled).size(18).on_toggle(|v| m(AdminMsg::RegEnabled(v)))].spacing(10).align_y(Alignment::Center),
            row![
                field_input("Max accounts (0 = unlimited)", &f.max_users, 180.0, |v| m(AdminMsg::RegMax(v))),
                field_input("Sign-ups per IP", &f.rate, 130.0, |v| m(AdminMsg::RegRate(v))),
                field_input("…per seconds", &f.window, 130.0, |v| m(AdminMsg::RegWindow(v))),
            ]
            .spacing(14),
            row![
                field_input("Invite code (empty = none needed)", &f.invite, 260.0, |v| m(AdminMsg::RegInvite(v))),
                column![space().height(18), small_btn("Generate", Tone::Dark, Some(m(AdminMsg::RegGenerate)))],
                space().width(Fill),
                column![space().height(18), small_btn("Save limits", Tone::Accent, Some(m(AdminMsg::RegSave)))],
            ]
            .spacing(10),
        ]
        .spacing(14),
    );

    let codes: El = match loading(&st.codes, "access codes") {
        Err(e) => e,
        Ok(c) => {
            let mut list: Column<'a, Message> = column![].spacing(4);
            for code in &c.codes {
                let used = code.used_by.is_some();
                list = list.push(
                    container(
                        row![
                            text(code.code.clone()).size(12.5).font(Font::MONOSPACE).color(if used { TEXT_SECONDARY } else { Color::WHITE }).width(Length::FillPortion(2)),
                            text(code.used_by.as_ref().map(|u| format!("used by @{u}")).unwrap_or_else(|| code.invited_by.as_ref().map(|i| format!("for @{i}")).unwrap_or_else(|| "unused".into()))).size(12).color(TEXT_SECONDARY).width(Length::FillPortion(2)),
                            text(code.created_at.as_deref().map(when).unwrap_or_default()).size(12).color(TEXT_SECONDARY).width(110),
                            button(icon(icons::copy(), 12, MUTED)).padding(5).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(Message::CopyText(code.code.clone(), "Code")),
                            button(icon(icons::trash(), 12, DANGER_TEXT)).padding(5).style(ghost(6.0, rgba(0xef4444, 0.12))).on_press_maybe((!used).then(|| m(AdminMsg::CodeDelete(code.code.clone())))),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .padding([5, 10])
                    .style(boxed(rgba(0xffffff, 0.02), Color::TRANSPARENT, 6.0)),
                );
            }
            column![text(format!("{} unused of {}", c.unused, c.total)).size(12).color(TEXT_SECONDARY), list].spacing(8).into()
        }
    };
    let mut filters: Row<'a, Message> = row![].spacing(4);
    for f in ["unused", "used", "all"] {
        filters = filters.push(chip_btn(f.into(), st.codes_filter == f, m(AdminMsg::CodesFilter(f))));
    }
    let codes_card = card(
        column![
            row![text("Access codes").size(15).font(SEMIBOLD).color(Color::WHITE), space().width(Fill), filters].align_y(Alignment::Center),
            row![
                field_input("How many", &st.code_count, 80.0, |v| m(AdminMsg::CodeCount(v))),
                field_input("For a username (optional)", &st.code_for, 200.0, |v| m(AdminMsg::CodeFor(v))),
                column![space().height(18), small_btn("Create codes", Tone::Accent, Some(m(AdminMsg::CodesCreate)))],
            ]
            .spacing(10),
            codes,
        ]
        .spacing(12),
    );

    let wait: El = match loading(&st.waitlist, "the waitlist") {
        Err(e) => e,
        Ok(w) => {
            let mut list: Column<'a, Message> = column![].spacing(4);
            for x in &w.entries {
                let id = x.id;
                let invited = x.invited_at.is_some();
                list = list.push(
                    container(
                        row![
                            checkbox(st.picked.contains(&id)).size(14).style(tick).on_toggle_maybe((!invited).then_some(move |v| m(AdminMsg::WaitPick(id, v)))),
                            text(x.email.clone()).size(12.5).color(Color::WHITE).width(Length::FillPortion(3)),
                            text(if invited { format!("invited {}", x.invited_at.as_deref().map(when).unwrap_or_default()) } else { format!("since {}", x.created_at.as_deref().map(when).unwrap_or_default()) }).size(12).color(TEXT_SECONDARY).width(Length::FillPortion(2)),
                            text(x.access_code.clone().unwrap_or_default()).size(12).font(Font::MONOSPACE).color(TEXT_SECONDARY).width(Length::FillPortion(2)),
                            button(icon(icons::trash(), 12, DANGER_TEXT)).padding(5).style(ghost(6.0, rgba(0xef4444, 0.12))).on_press(m(AdminMsg::WaitDelete(id))),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .padding([5, 10])
                    .style(boxed(rgba(0xffffff, 0.02), Color::TRANSPARENT, 6.0)),
                );
            }
            column![text(format!("{} waiting of {}", w.pending, w.total)).size(12).color(TEXT_SECONDARY), list].spacing(8).into()
        }
    };
    let wait_card = card(
        column![
            row![text("Waitlist").size(15).font(SEMIBOLD).color(Color::WHITE), space().width(Fill), small_btn(format!("Invite {} ticked", st.picked.len()), Tone::Accent, (!st.picked.is_empty()).then(|| m(AdminMsg::WaitInvite)))].align_y(Alignment::Center),
            wait,
        ]
        .spacing(12),
    );
    column![reg, defaults_card(st), codes_card, wait_card].spacing(14).into()
}

fn orgs<'a>(st: &'a State) -> El<'a> {
    let list = match loading(&st.orgs, "organizations") {
        Ok(l) => l,
        Err(e) => return e,
    };
    if list.is_empty() {
        return container(text("No organizations yet").size(13).color(TEXT_SECONDARY)).center_x(Fill).padding(40).into();
    }
    let mut col: Column<'a, Message> = column![].spacing(10);
    for o in list {
        let action = if o.verified { small_btn("Take off", Tone::Dark, Some(m(AdminMsg::OrgVerify(o.id, false)))) } else { small_btn("Verify", Tone::Accent, Some(m(AdminMsg::OrgVerify(o.id, true)))) };
        let mut about = format!("by @{} · {} / {} members", o.owner_username, o.members, o.max_members);
        if let Some(w) = o.website.as_deref().filter(|w| !w.is_empty()) {
            about.push_str(&format!(" · {w}"));
        }
        let mut info = column![
            row![text(o.name.clone()).size(14).font(BOLD).color(Color::WHITE), if o.verified { badge("VERIFIED", hex(0x93c5fd)) } else { badge("NOT VERIFIED", MUTED) }].spacing(8).align_y(Alignment::Center),
            text(about).size(12).color(TEXT_SECONDARY),
        ]
        .spacing(3)
        .width(Fill);
        if !o.description.is_empty() {
            info = info.push(text(o.description.clone()).size(12).color(TEXT_BODY));
        }
        let id = o.id;
        let typed = st.org_caps.get(&id).cloned().unwrap_or_else(|| o.max_members.to_string());
        let cap = row![
            text_input("10", &typed).on_input(move |v| m(AdminMsg::OrgCap(id, v))).on_submit(m(AdminMsg::OrgCapSave(id))).size(12.5).padding([5, 8]).width(70).style(field),
            small_btn("Set cap", Tone::Dark, st.org_caps.contains_key(&id).then(|| m(AdminMsg::OrgCapSave(id)))),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        col = col.push(card(row![crate::components::ui::avatar_pic(&o.name, o.logo.as_deref(), 40.0, None), info, cap, action].spacing(12).align_y(Alignment::Center)));
    }
    scrollable(col).height(Fill).style(thin_scroll).into()
}

fn apps<'a>(st: &'a State) -> El<'a> {
    let list = match loading(&st.apps, "mini-apps") {
        Ok(l) => l,
        Err(e) => return e,
    };
    if list.is_empty() {
        return container(text("No mini-apps registered yet").size(13).color(TEXT_SECONDARY)).center_x(Fill).padding(40).into();
    }
    let mut col: Column<'a, Message> = column![].spacing(10);
    for a in list {
        let id = a.id;
        let (label, c) = match a.status {
            AppStatus::Active => ("ACTIVE", GREEN),
            AppStatus::Pending => ("IN REVIEW", AMBER_TEXT),
            AppStatus::Rejected => ("REJECTED", DANGER_TEXT),
            AppStatus::Disabled => ("OFF", MUTED),
            AppStatus::Draft => ("DRAFT", MUTED),
            AppStatus::Unknown => ("—", MUTED),
        };
        let mut actions = row![].spacing(6);
        match a.status {
            AppStatus::Pending => {
                actions = actions.push(small_btn("Approve", Tone::Accent, Some(m(AdminMsg::AppStatus(id, "active"))))).push(small_btn("Reject", Tone::Dark, Some(m(AdminMsg::AppStatus(id, "rejected")))));
            }
            AppStatus::Active => actions = actions.push(small_btn("Switch off", Tone::Dark, Some(m(AdminMsg::AppStatus(id, "disabled"))))),
            AppStatus::Disabled | AppStatus::Rejected => actions = actions.push(small_btn("Approve", Tone::Dark, Some(m(AdminMsg::AppStatus(id, "active"))))),
            _ => {}
        }
        actions = actions.push(small_btn("A year free", Tone::Dark, Some(m(AdminMsg::AppGrant(id))))).push(small_btn("Delete", Tone::Danger, Some(m(AdminMsg::AppRemove(id)))));
        let initial = a.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
        col = col.push(card(
            column![
                row![
                    container(text(initial).size(16).font(BOLD).color(INDIGO_SOFT)).width(40).height(40).center(40).style(boxed(rgba(0x6366f1, 0.15), Color::TRANSPARENT, 10.0)),
                    column![
                        row![text(a.name.clone()).size(14).font(BOLD).color(Color::WHITE), text(format!("/{}", a.slug)).size(12).font(Font::MONOSPACE).color(TEXT_SECONDARY), badge(label, c)].spacing(8).align_y(Alignment::Center),
                        text(format!("{} · by {}", a.origin, a.developer.clone().unwrap_or_else(|| "Accordo".into()))).size(12).color(TEXT_SECONDARY),
                    ]
                    .spacing(3)
                    .width(Fill),
                    actions,
                ]
                .spacing(12)
                .align_y(Alignment::Center),
                text(a.description.clone()).size(12.5).color(TEXT_BODY),
                text(format!("Permissions: {}", if a.permissions.is_empty() { "none".to_string() } else { a.permissions.join(", ") })).size(11.5).color(FAINT),
            ]
            .spacing(8),
        ));
        if let Some(note) = &a.review_note {
            col = col.push(text(format!("Review note: {note}")).size(11.5).color(AMBER_TEXT));
        }
    }
    col.into()
}

fn level_color(l: &str) -> Color {
    match l {
        "success" => hex(0x34d399),
        "warning" => hex(0xfbbf24),
        "critical" => hex(0xf87171),
        _ => hex(0x38bdf8),
    }
}

fn notifications<'a>(st: &'a State) -> El<'a> {
    let c = &st.compose;
    let users = st.users.as_ref().and_then(|r| r.as_ref().ok()).map_or(0, |l| l.iter().filter(|u| u.is_active).count());
    let mut levels: Row<'a, Message> = row![].spacing(6);
    for l in ["info", "success", "warning", "critical"] {
        levels = levels.push(chip_btn(l.into(), c.level == l, m(AdminMsg::NoteLevel(l))));
    }
    let mut audience = column![row![chip_btn("All users".into(), c.all, m(AdminMsg::NoteAll(true))), chip_btn("Specific users".into(), !c.all, m(AdminMsg::NoteAll(false)))].spacing(6)].spacing(8);
    if c.all {
        audience = audience.push(text(format!("Goes to all {users} active accounts.")).size(12).color(TEXT_SECONDARY));
    } else {
        audience = audience.push(row![text_input("Usernames, emails or IDs — commas or new lines", &c.who).on_input(|v| m(AdminMsg::NoteWho(v))).size(13).padding([7, 10]).style(field), small_btn("Check", Tone::Dark, Some(m(AdminMsg::NoteResolve)))].spacing(8).align_y(Alignment::Center));
        match &c.resolved {
            Some(Ok(r)) => {
                let found: Vec<String> = r.resolved.iter().map(|x| format!("@{}", x.username.clone().unwrap_or_else(|| x.id.to_string()))).collect();
                audience = audience.push(text(format!("Found {}: {}", found.len(), found.join(", "))).size(12).color(GREEN));
                if !r.unresolved.is_empty() {
                    audience = audience.push(text(format!("Not found: {}", r.unresolved.join(", "))).size(12).color(DANGER_TEXT));
                }
            }
            Some(Err(e)) => audience = audience.push(text(e.clone()).size(12).color(DANGER_TEXT)),
            None => {}
        }
    }
    let compose = card(
        column![
            text("New notification").size(15).font(SEMIBOLD).color(Color::WHITE),
            column![caption("Audience"), audience].spacing(6),
            column![caption("Level"), levels].spacing(6),
            column![caption("Title"), text_input("Short headline", &c.title).on_input(|v| m(AdminMsg::NoteTitle(v))).size(13.5).padding([8, 10]).style(field)].spacing(6),
            column![caption("Message"), text_input("What people should know", &c.body).on_input(|v| m(AdminMsg::NoteBody(v))).size(13.5).padding([12, 10]).style(field)].spacing(6),
            column![caption("Link (optional)"), text_input("https://…", &c.link).on_input(|v| m(AdminMsg::NoteLink(v))).size(13).padding([7, 10]).style(field)].spacing(6),
            row![space().width(Fill), small_btn(if st.busy { "Sending…" } else { "Send notification" }, Tone::Accent, (!st.busy).then(|| m(AdminMsg::NoteSend)))],
        ]
        .spacing(14),
    );
    let sent: El = match loading(&st.notes, "sent notifications") {
        Err(e) => e,
        Ok(list) if list.is_empty() => text("Nothing sent yet").size(12.5).color(TEXT_SECONDARY).into(),
        Ok(list) => {
            let mut col: Column<'a, Message> = column![].spacing(8);
            for a in list {
                let lc = level_color(&a.level);
                col = col.push(
                    container(
                        row![
                            container(space()).width(4).height(36).style(boxed(lc, Color::TRANSPARENT, 2.0)),
                            column![
                                row![text(a.title.clone()).size(13).font(BOLD).color(if a.retracted { TEXT_SECONDARY } else { Color::WHITE }), badge(a.level.to_uppercase(), lc)].spacing(8).align_y(Alignment::Center),
                                text(crate::components::ui::truncate(&a.body, 140)).size(12).color(TEXT_SECONDARY),
                                text(format!("{} · {} recipient{} · {}{}", when_time(&a.created_at), a.recipients_count, if a.recipients_count == 1 { "" } else { "s" }, if a.target_type == "all" { "everyone" } else { "specific users" }, if a.retracted { " · retracted" } else { "" })).size(11).color(FAINT),
                            ]
                            .spacing(3)
                            .width(Fill),
                            small_btn("Retract", Tone::Dark, (!a.retracted).then(|| m(AdminMsg::NoteRetract(a.id)))),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .padding([8, 12])
                    .style(boxed(rgba(0xffffff, 0.02), BORDER, 8.0)),
                );
            }
            col.into()
        }
    };
    column![compose, card(column![text("Sent").size(15).font(SEMIBOLD).color(Color::WHITE), sent].spacing(12))].spacing(14).into()
}

fn report_card<'a>(title: String, meta: String, body: Option<String>, status: &str, resolution: Option<&str>, actions: Vec<El<'a>>) -> El<'a> {
    let mut col = column![row![text(title).size(13.5).font(BOLD).color(Color::WHITE).width(Fill), text(meta).size(11.5).color(FAINT)].spacing(8).align_y(Alignment::Center)].spacing(8);
    if let Some(b) = body.filter(|b| !b.trim().is_empty()) {
        col = col.push(container(text(crate::components::ui::truncate(&b, 400)).size(12.5).color(TEXT_BODY)).width(Fill).padding([8, 10]).style(boxed(rgba(0x000000, 0.25), Color::TRANSPARENT, 6.0)));
    }
    if status == "open" {
        let mut r: Row<'a, Message> = row![].spacing(6);
        for a in actions {
            r = r.push(a);
        }
        col = col.push(r);
    } else {
        col = col.push(text(if status == "dismissed" { "Dismissed".to_string() } else { format!("Resolved: {}", resolution.unwrap_or("").replace('_', " ").replace('+', " + ")) }).size(12).color(TEXT_SECONDARY));
    }
    card(col)
}

fn reports<'a>(st: &'a State) -> El<'a> {
    let mut kinds: Row<'a, Message> = row![].spacing(6);
    for (k, l, n) in [(ReportKind::Messages, "Messages", st.open_counts[0]), (ReportKind::Rooms, "Rooms", st.open_counts[1]), (ReportKind::Campaigns, "Campaigns", st.open_counts[2])] {
        kinds = kinds.push(chip_btn(if n > 0 { format!("{l} ({n})") } else { l.to_string() }, st.report_kind == k, m(AdminMsg::ReportKind(k))));
    }
    let mut filters: Row<'a, Message> = row![].spacing(4);
    for (f, l) in [(ReportFilter::Open, "Open"), (ReportFilter::Resolved, "Resolved"), (ReportFilter::Dismissed, "Dismissed")] {
        filters = filters.push(chip_btn(l.into(), st.report_filter == f, m(AdminMsg::ReportFilter(f))));
    }
    let who = |u: &Option<crate::api::admin::UserRef>| u.as_ref().map(|u| format!("@{}", u.username.clone().unwrap_or_else(|| u.id.to_string()))).unwrap_or_else(|| "—".into());
    let body: El = match st.report_kind {
        ReportKind::Messages => match loading(&st.msg_reports, "reports") {
            Err(e) => e,
            Ok(l) if l.is_empty() => container(text("No reports here").size(13).color(TEXT_SECONDARY)).center_x(Fill).padding(30).into(),
            Ok(l) => {
                let mut col: Column<'a, Message> = column![].spacing(10);
                for r in l {
                    let id = r.id;
                    let gone = !r.message_exists;
                    let actions = vec![
                        small_btn("Delete + suspend", Tone::Danger, (!gone).then(|| m(AdminMsg::ActMessage(id, "delete_and_suspend")))),
                        small_btn("Delete message", Tone::Dark, (!gone).then(|| m(AdminMsg::ActMessage(id, "delete_message")))),
                        small_btn("Suspend sender", Tone::Dark, (r.reported_user.as_ref().and_then(|u| u.is_active) != Some(false)).then(|| m(AdminMsg::ActMessage(id, "suspend_sender")))),
                        small_btn("Dismiss", Tone::Dark, Some(m(AdminMsg::ActMessage(id, "dismiss")))),
                    ];
                    let title = format!("{} — message by {}{}", r.reason.replace('_', " "), who(&r.reported_user), if r.report_count > 1 { format!(" · {} reports", r.report_count) } else { String::new() });
                    let meta = format!("by {} · {}", who(&r.reporter), when_time(&r.created_at));
                    let snap = r.content_snapshot.clone().map(|c| if gone { format!("{c}  (already deleted)") } else { c });
                    col = col.push(report_card(title, meta, snap.or_else(|| r.comment.clone()), &r.status, r.resolution.as_deref(), actions));
                }
                col.into()
            }
        },
        ReportKind::Rooms => match loading(&st.room_reports, "reports") {
            Err(e) => e,
            Ok(l) if l.is_empty() => container(text("No reports here").size(13).color(TEXT_SECONDARY)).center_x(Fill).padding(30).into(),
            Ok(l) => {
                let mut col: Column<'a, Message> = column![].spacing(10);
                for r in l {
                    let id = r.id;
                    let gone = !r.room_exists;
                    let actions = vec![
                        small_btn("Delete room + suspend", Tone::Danger, (!gone).then(|| m(AdminMsg::ActRoom(id, "delete_and_suspend")))),
                        small_btn("Delete room", Tone::Dark, (!gone).then(|| m(AdminMsg::ActRoom(id, "delete_room")))),
                        small_btn("Suspend owner", Tone::Dark, (r.owner.is_some() && r.owner.as_ref().and_then(|u| u.is_active) != Some(false)).then(|| m(AdminMsg::ActRoom(id, "suspend_owner")))),
                        small_btn("Dismiss", Tone::Dark, Some(m(AdminMsg::ActRoom(id, "dismiss")))),
                    ];
                    let title = format!("{} — {} ({} members) by {}", r.reason.replace('_', " "), r.room_title.clone().unwrap_or_else(|| r.room_key.clone()), r.member_count, who(&r.owner));
                    let meta = format!("by {} · {}", who(&r.reporter), when_time(&r.created_at));
                    col = col.push(report_card(title, meta, r.comment.clone().or_else(|| r.room_topic.clone()), &r.status, r.resolution.as_deref(), actions));
                }
                col.into()
            }
        },
        ReportKind::Campaigns => match loading(&st.campaign_reports, "reports") {
            Err(e) => e,
            Ok(l) if l.is_empty() => container(text("No reports here").size(13).color(TEXT_SECONDARY)).center_x(Fill).padding(30).into(),
            Ok(l) => {
                let mut col: Column<'a, Message> = column![].spacing(10);
                for r in l {
                    let id = r.id;
                    let actions = vec![
                        small_btn("Hide + suspend creator", Tone::Danger, Some(m(AdminMsg::ActCampaign(id, "hide_and_suspend")))),
                        small_btn("Hide campaign", Tone::Dark, Some(m(AdminMsg::ActCampaign(id, "hide")))),
                        small_btn("Dismiss", Tone::Dark, Some(m(AdminMsg::ActCampaign(id, "dismiss")))),
                    ];
                    let title = format!(
                        "{} — {} by @{} · {} {} raised{}",
                        r.reason.replace('_', " "),
                        r.title.clone().unwrap_or_else(|| r.campaign_id.clone()),
                        r.creator_username.clone().unwrap_or_default(),
                        r.raised.unwrap_or(0.0),
                        r.token_symbol.clone().unwrap_or_default(),
                        if r.hidden == Some(true) { " · hidden" } else { "" }
                    );
                    let meta = format!("by @{} · {}", r.reporter.username.clone().unwrap_or_else(|| r.reporter.id.to_string()), when_time(&r.created_at));
                    col = col.push(report_card(title, meta, r.comment.clone(), &r.status, r.resolution.as_deref(), actions));
                }
                col.into()
            }
        },
    };
    column![row![kinds, space().width(Fill), filters].align_y(Alignment::Center), body].spacing(14).into()
}

fn nav<'a>(st: &'a State, tab: AdminTab, handle: iced::widget::svg::Handle, label: &'a str, count: Option<usize>, alert: bool) -> El<'a> {
    let on = st.tab == tab;
    let mut r = row![icon(handle, 15, if on { Color::WHITE } else { MUTED }), text(label).size(13).font(if on { BOLD } else { MEDIUM }).color(if on { Color::WHITE } else { TEXT_BODY }), space().width(Fill)].spacing(10).align_y(Alignment::Center);
    if let Some(n) = count {
        r = r.push(if alert && n > 0 { container(text(n.to_string()).size(10.5).font(BOLD).color(Color::WHITE)).padding([1, 7]).style(boxed(hex(0xdc2626), Color::TRANSPARENT, 9.0)).into() } else { El::from(text(n.to_string()).size(11.5).color(TEXT_SECONDARY)) });
    }
    button(r).width(Fill).padding([8, 10]).style(selectable(on, rgba(0xffffff, 0.08), 8.0)).on_press(m(AdminMsg::Tab(tab))).into()
}

fn count<T>(v: &Option<Result<Vec<T>, String>>) -> Option<usize> {
    v.as_ref().and_then(|r| r.as_ref().ok()).map(Vec::len)
}

pub fn view<'a>(st: &'a State) -> El<'a> {
    let reports_n = st.open_counts.iter().sum::<usize>();
    let reg_open = st.reg.as_ref().and_then(|r| r.as_ref().ok()).map(|r| r.enabled);
    let side = container(
        column![
            row![
                container(icon(icons::shield(), 18, INDIGO_SOFT)).width(36).height(36).center(36).style(boxed(rgba(0x6366f1, 0.14), rgba(0x6366f1, 0.3), 10.0)),
                column![text("Admin Console").size(16).font(BOLD).color(Color::WHITE), text("Global overview of all Accordo data").size(11).color(TEXT_SECONDARY)].spacing(2),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            column![
                nav(st, AdminTab::Overview, icons::chart_bar(), "Overview", None, false),
                nav(st, AdminTab::Users, icons::users(), "Users", count(&st.users), false),
                nav(st, AdminTab::Invoices, icons::receipt(), "Invoices", count(&st.invoices), false),
                nav(st, AdminTab::Cheques, icons::wallet(), "Cheques", count(&st.cheques), false),
                nav(st, AdminTab::Access, icons::shield_check(), "Access & Limits", None, false),
                nav(st, AdminTab::Apps, icons::app_window(), "Mini-apps", count(&st.apps), false),
                nav(st, AdminTab::Orgs, icons::briefcase(), "Organizations", count(&st.orgs), false),
                nav(st, AdminTab::Notifications, icons::megaphone(), "Notifications", None, false),
                nav(st, AdminTab::Reports, icons::shield_alert(), "Reports", Some(reports_n), true),
            ]
            .spacing(3),
            space().height(Fill),
            match reg_open {
                Some(open) => El::from(row![container(space()).width(8).height(8).style(boxed(if open { hex(0x57f287) } else { hex(0xed4245) }, Color::TRANSPARENT, 4.0)), text(if open { "Registration open" } else { "Registration closed" }).size(11.5).color(TEXT_SECONDARY)].spacing(8).align_y(Alignment::Center)),
                None => space().into(),
            },
        ]
        .spacing(18),
    )
    .width(240)
    .height(Fill)
    .padding([20, 12])
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0));

    let (title, sub) = match st.tab {
        AdminTab::Overview => ("Overview", "Accounts, activity and registration at a glance"),
        AdminTab::Users => ("Users", "Click a row for the account's card"),
        AdminTab::Invoices => ("Invoices", "Every invoice; click one for its record"),
        AdminTab::Cheques => ("Cheques", "Every cheque; click one for its record"),
        AdminTab::Access => ("Access & Limits", "Who can sign up, and how"),
        AdminTab::Apps => ("Mini-apps", "Review and switch apps on or off"),
        AdminTab::Orgs => ("Organizations", "Verify the real ones — the check people trust"),
        AdminTab::Notifications => ("Notifications", "Announcements to everyone or to some"),
        AdminTab::Reports => ("Reports", "What people flagged"),
    };
    let mut head = row![column![text(title).size(20).font(BOLD).color(Color::WHITE), text(sub).size(12).color(TEXT_SECONDARY)].spacing(3), space().width(Fill)].spacing(10).align_y(Alignment::Center);
    if matches!(st.tab, AdminTab::Users | AdminTab::Invoices | AdminTab::Cheques) {
        head = head.push(container(row![icon(icons::search(), 13, MUTED), text_input("Search IDs, names, emails…", &st.search).on_input(|q| m(AdminMsg::Search(q))).size(13).padding([7, 0]).style(bare_input)].spacing(6).align_y(Alignment::Center)).width(280).padding([0, 10]).style(boxed(rgba(0xffffff, 0.03), BORDER, 8.0)));
    }
    head = head
        .push(button(icon(icons::refresh_cw(), 15, MUTED)).padding(7).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(m(AdminMsg::Refresh)))
        .push(button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(m(AdminMsg::Close)));
    let body: El = match st.tab {
        AdminTab::Overview => overview(st),
        AdminTab::Users => users(st),
        AdminTab::Invoices => records_tab(st, true),
        AdminTab::Cheques => records_tab(st, false),
        AdminTab::Access => access(st),
        AdminTab::Apps => apps(st),
        AdminTab::Orgs => orgs(st),
        AdminTab::Notifications => notifications(st),
        AdminTab::Reports => reports(st),
    };
    let main = column![
        container(head).padding(Padding { top: 18.0, right: 16.0, bottom: 12.0, left: 26.0 }),
        container(space().width(Fill).height(1)).style(rule),
        scrollable(container(body).width(Fill).padding([18, 26])).height(Fill).style(thin_scroll),
    ]
    .width(Fill);
    let window = container(row![side, main]).width(Length::Fixed(1240.0)).height(Length::Fixed(780.0)).style(modal);
    let base = overlay::modal(window, m(AdminMsg::Close));
    match st.profile.as_ref().and_then(|p| st.user(p.user_id).map(|u| (p, u))) {
        Some((p, u)) => iced::widget::stack![base, profile(st, p, u)].into(),
        None => base,
    }
}

// ── one user's card ──

fn line<'a>(handle: iced::widget::svg::Handle, label: &'a str, value: El<'a>) -> El<'a> {
    row![row![icon(handle, 13, MUTED), text(label).size(12.5).color(TEXT_SECONDARY)].spacing(7).align_y(Alignment::Center).width(140), value].spacing(10).align_y(Alignment::Center).into()
}

fn stepper<'a>(value: i64, max: i64, on: impl Fn(i64) -> AdminMsg) -> El<'a> {
    row![
        button(icon(icons::minus(), 12, TEXT_BODY)).padding(4).style(ghost(6.0, rgba(0xffffff, 0.08))).on_press_maybe((value > 0).then(|| m(on(value - 1)))),
        text(value.to_string()).size(13).font(BOLD).color(if value == 0 { DANGER_TEXT } else { TEXT_BODY }).width(26).align_x(iced::alignment::Horizontal::Center),
        button(icon(icons::plus(), 12, TEXT_BODY)).padding(4).style(ghost(6.0, rgba(0xffffff, 0.08))).on_press_maybe((value < max).then(|| m(on(value + 1)))),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn num_input<'a>(value: &str, max: i64, on: impl Fn(String) -> AdminMsg + 'a) -> El<'a> {
    let ok = value.trim().parse::<i64>().is_ok_and(|n| (0..=max).contains(&n));
    text_input("", value)
        .on_input(move |v| m(on(v)))
        .size(12.5)
        .padding([4, 8])
        .width(70)
        .style(move |t: &iced::Theme, s| {
            let mut f = field(t, s);
            if !ok {
                f.border.color = DANGER_TEXT;
            }
            f
        })
        .into()
}

fn profile<'a>(st: &'a State, p: &'a Profile, u: &'a AdminUser) -> El<'a> {
    let c: &Control = &p.control;
    let online = st.online.contains(&u.id);
    let initial = u.username.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    let mut status = row![badge(if u.is_active { "ACTIVE" } else { "SUSPENDED" }, if u.is_active { GREEN } else { DANGER_TEXT })].spacing(6);
    if u.is_admin {
        status = status.push(badge("ADMIN", INDIGO_SOFT));
    }
    if online {
        status = status.push(badge("ONLINE", hex(0x57f287)));
    }
    let id = u.id;
    let left = container(
        column![
            container(text(initial).size(30).font(EXTRABOLD).color(Color::WHITE)).width(72).height(72).center(72).style(boxed(INDIGO, Color::TRANSPARENT, 36.0)),
            column![text(u.display_name.clone().filter(|d| !d.is_empty()).unwrap_or_else(|| u.username.clone())).size(18).font(BOLD).color(Color::WHITE), text(format!("@{} · #{}", u.username, u.id)).size(12).color(TEXT_SECONDARY)].spacing(2),
            status,
            column![
                line(icons::mail(), "Email", text(u.email.clone().unwrap_or_else(|| "—".into())).size(12.5).color(TEXT_BODY).into()),
                line(icons::calendar(), "Joined", text(when(&u.created_at)).size(12.5).color(TEXT_BODY).into()),
                line(icons::clock(), "Last seen", text(u.last_seen_at.as_deref().map(when_time).unwrap_or_else(|| "—".into())).size(12.5).color(TEXT_BODY).into()),
                line(icons::receipt(), "Activity", text(format!("{} invoices · {} cheques", u.invoice_count, u.cheque_count)).size(12.5).color(TEXT_BODY).into()),
                line(icons::message_circle(), "Discord", text(u.discord_username.as_ref().map(|d| format!("@{d}")).unwrap_or_else(|| "—".into())).size(12.5).color(TEXT_BODY).into()),
                line(icons::key(), "API-key login", toggler(u.api_login).size(16).on_toggle(move |v| m(AdminMsg::SetApiLogin(id, v))).into()),
            ]
            .spacing(10),
            space().height(Fill),
            small_btn(if u.is_active { "Suspend account" } else { "Activate account" }, if u.is_active { Tone::Danger } else { Tone::Accent }, (!u.is_admin || !u.is_active).then(|| m(AdminMsg::SetActive(id, !u.is_active)))),
        ]
        .spacing(16),
    )
    .width(300)
    .height(Fill)
    .padding([24, 20])
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0));

    let mut plans: Column<'a, Message> = column![].spacing(8);
    for (i, name) in PLANS.iter().enumerate() {
        let active = chrono::NaiveDate::parse_from_str(c.plans[i].trim(), "%Y-%m-%d").is_ok_and(|d| d >= chrono::Local::now().date_naive());
        plans = plans.push(line(
            icons::sparkles(),
            match i {
                0 => "Basic",
                1 => "Lite",
                2 => "Pro",
                _ => "Enterprise",
            },
            row![
                text_input("YYYY-MM-DD", &c.plans[i]).on_input(move |v| m(AdminMsg::Plan(i, v))).size(12.5).padding([4, 8]).width(120).style(field),
                small_btn("+30 d", Tone::Dark, Some(m(AdminMsg::PlanAdd30(i)))),
                small_btn("Off", Tone::Dark, (!c.plans[i].is_empty()).then(|| m(AdminMsg::Plan(i, String::new())))),
                text(if active { "active" } else if c.plans[i].is_empty() { "none" } else { "expired" }).size(11.5).color(if active { GREEN } else { TEXT_SECONDARY }),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
        ));
        let _ = name;
    }
    let changed = c.changed(u);
    let control = card(
        column![
            row![text("Control").size(15).font(SEMIBOLD).color(Color::WHITE), space().width(Fill), small_btn("Reset", Tone::Dark, changed.then(|| m(AdminMsg::ResetControl))), small_btn(if p.saving { "Saving…" } else { "Save" }, Tone::Accent, (changed && !p.saving).then(|| m(AdminMsg::SaveControl)))].spacing(6).align_y(Alignment::Center),
            admin_limits(u, c),
            line(icons::file_text(), "Articles", toggler(c.articles).size(16).on_toggle(|v| m(AdminMsg::Articles(v))).into()),
            line(icons::tag(), "Role", row![text_input("member", &c.role).on_input(|v| m(AdminMsg::Role(v))).size(12.5).padding([5, 8]).width(160).style(field), text("a label on their profile — no rights").size(11.5).color(TEXT_SECONDARY)].spacing(10).align_y(Alignment::Center).into()),
            line(icons::code(), "Developer", if u.is_admin { text("always (admin)").size(11.5).color(TEXT_SECONDARY).into() } else { toggler(c.developer).size(16).on_toggle(|v| m(AdminMsg::Developer(v))).into() }),
            container(space().width(Fill).height(1)).style(rule),
            caption("Plans — paid until"),
            plans,
        ]
        .spacing(12),
    );

    let rooms: El = match loading(&p.rooms, "rooms") {
        Err(e) => e,
        Ok(list) if list.is_empty() => text("Owns no rooms").size(12.5).color(TEXT_SECONDARY).into(),
        Ok(list) => {
            let mut col: Column<'a, Message> = column![].spacing(6);
            for r in list {
                let key = r.room_key.clone();
                let cap_value = p.caps.get(&r.room_key).cloned().unwrap_or_else(|| r.admin_cap.map(|c| c.to_string()).unwrap_or_default());
                let k2 = key.clone();
                let k3 = key.clone();
                col = col.push(
                    container(
                        row![
                            column![
                                row![text(r.title.clone().unwrap_or_else(|| r.room_key.clone())).size(13).font(SEMIBOLD).color(Color::WHITE), badge(r.chat_type.to_uppercase(), MUTED)].spacing(6).align_y(Alignment::Center),
                                text(format!("{} / {} members · since {}{}", r.members, r.cap, when(&r.created_at), r.frozen.as_ref().map(|f| format!(" · frozen {}", when(&f.at))).unwrap_or_default())).size(11.5).color(if r.frozen.is_some() { SKY } else { TEXT_SECONDARY }),
                            ]
                            .spacing(2)
                            .width(Fill),
                            text_input(&format!("cap {}", r.default_cap), &cap_value).on_input(move |v| m(AdminMsg::RoomCap(k2.clone(), v))).size(12).padding([4, 8]).width(80).style(field),
                            small_btn("Set cap", Tone::Dark, Some(m(AdminMsg::SaveRoomCap(k3)))),
                            small_btn(if r.frozen.is_some() { "Unfreeze" } else { "Freeze" }, Tone::Dark, Some(m(AdminMsg::Freeze(key, r.frozen.is_none())))),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .padding([8, 10])
                    .style(boxed(rgba(0xffffff, 0.02), BORDER, 8.0)),
                );
            }
            col.into()
        }
    };
    let right = column![
        container(row![space().width(Fill), button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(m(AdminMsg::CloseUser))]).padding(Padding { top: 12.0, right: 12.0, bottom: 0.0, left: 0.0 }),
        scrollable(container(column![control, card(column![text("Rooms they own").size(15).font(SEMIBOLD).color(Color::WHITE), rooms].spacing(12))].spacing(14)).width(Fill).padding(Padding { top: 0.0, right: 22.0, bottom: 20.0, left: 22.0 })).height(Fill).style(thin_scroll),
    ]
    .width(Fill);
    let window = container(row![left, right]).width(Length::Fixed(980.0)).height(Length::Fixed(680.0)).style(modal);
    overlay::modal(window, m(AdminMsg::CloseUser))
}
