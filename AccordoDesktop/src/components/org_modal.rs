// SPDX-License-Identifier: GPL-3.0-or-later
//! The organizations' windows, opened from the rail: "New organization" (logo, name, description,
//! website) and one organization (its people with their titles; the owner manages them).

use iced::widget::{button, column, container, row, scrollable, space, stack, text, text_input, Column};
use iced::alignment::{Horizontal, Vertical};
use iced::{Alignment, Color, Fill};

use crate::app::orgs::{NewOrg, Orgs, Window};
use crate::app::{Message, OrgMsg};
use crate::api::orgs::OrgCard;
use crate::components::overlay;
use crate::components::ui::{avatar_pic, icon, El};
use crate::icons;
use crate::theme::*;

fn m(o: OrgMsg) -> Message {
    Message::Org(o)
}

fn btn<'a>(label: &'a str, tone: Tone, on: Option<OrgMsg>) -> El<'a> {
    button(text(label).size(12.5).font(SEMIBOLD)).padding([7, 14]).style(tone_button(tone)).on_press_maybe(on.map(m)).into()
}

fn label<'a>(t: &'a str) -> El<'a> {
    text(t).size(11.5).color(TEXT_SECONDARY).into()
}

fn header<'a>(title: &'a str, sub: String) -> El<'a> {
    row![
        column![text(title).size(17).font(BOLD).color(Color::WHITE), text(sub).size(12).color(TEXT_SECONDARY)].spacing(2).width(Fill),
        button(icon(icons::x(), 16, MUTED)).padding(2).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(m(OrgMsg::Close)),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// The logo with a camera badge: a click picks a file.
fn logo_picker<'a>(name: &str, logo: Option<&str>, target: Option<i64>, size: f32) -> El<'a> {
    let face: El = match logo {
        Some(l) => avatar_pic(if name.is_empty() { "?" } else { name }, Some(l), size, None),
        None => container(icon(icons::briefcase(), (size * 0.4) as u32, TEXT_SECONDARY)).width(size).height(size).center(size).style(boxed(rgba(0xffffff, 0.04), BORDER_STRONG, size / 2.0)).into(),
    };
    let badge = container(icon(icons::camera(), 11, Color::WHITE)).width(22).height(22).center(22).style(|_| iced::widget::container::Style {
        background: Some(INDIGO.into()),
        border: iced::Border { color: hex(0x13141c), width: 2.0, radius: 11.0.into() },
        ..Default::default()
    });
    button(stack![face, container(badge).width(size + 2.0).height(size + 2.0).align_x(Horizontal::Right).align_y(Vertical::Bottom)])
        .padding(0)
        .style(ghost(size / 2.0, rgba(0xffffff, 0.0)))
        .on_press(m(OrgMsg::PickLogo(target)))
        .into()
}

fn new_org<'a>(f: &'a NewOrg, st: &'a Orgs) -> El<'a> {
    let can = st.mine.as_ref().is_some_and(|x| x.can_create);
    let mut logo_col = column![logo_picker(&f.name, f.logo.as_deref(), None, 72.0)].spacing(6).align_x(Alignment::Center);
    if f.logo.is_some() {
        logo_col = logo_col.push(button(text("Remove").size(11).color(DANGER_TEXT)).padding([1, 6]).style(ghost(5.0, rgba(0xef4444, 0.1))).on_press(m(OrgMsg::ClearLogo)));
    }
    let fields = column![
        column![label("Name"), text_input("Northwind Labs", &f.name).on_input(|v| m(OrgMsg::Name(v))).on_submit(m(OrgMsg::Create)).size(13.5).padding([9, 11]).style(field)].spacing(5),
        column![label("About (optional)"), text_input("What you do, in a line", &f.description).on_input(|v| m(OrgMsg::Description(v))).size(13).padding([9, 11]).style(field)].spacing(5),
        column![label("Website (optional)"), text_input("https://", &f.website).on_input(|v| m(OrgMsg::Website(v))).size(13).padding([9, 11]).style(field)].spacing(5),
    ]
    .spacing(12)
    .width(Fill);
    let mut col = column![header("New organization", "Your team, shown on its people's profiles".into()), row![logo_col, fields].spacing(18)].spacing(18);
    col = col.push(text("You'll be its owner: you add people by their username and give them a title. Accordo verifies real organizations — that's the check people see.").size(11.5).color(FAINT).line_height(1.45));
    if !can {
        let plan = st.mine.as_ref().map(|x| x.plan.as_str()).filter(|p| !p.is_empty()).unwrap_or("enterprise");
        let plan = plan[..1].to_uppercase() + &plan[1..];
        col = col.push(container(text(format!("Creating an organization comes with the {plan} subscription.")).size(12).color(AMBER_TEXT)).width(Fill).padding([8, 10]).style(boxed(rgba(0xf59e0b, 0.08), rgba(0xf59e0b, 0.3), 8.0)));
    }
    col = col.push(row![
        space().width(Fill),
        btn("Cancel", Tone::Dark, Some(OrgMsg::Close)),
        btn(if st.busy { "Creating…" } else { "Create" }, Tone::Accent, (can && !st.busy && f.name.trim().chars().count() >= 2).then_some(OrgMsg::Create)),
    ]
    .spacing(8));
    col.into()
}

fn org_window<'a>(o: &'a OrgCard, who: &'a str, title: &'a str, st: &'a Orgs, me: i64) -> El<'a> {
    let owner = o.owner_id == me;
    let busy = st.busy;
    let mut name_line = row![text(o.name.as_str()).size(18).font(BOLD).color(Color::WHITE)].spacing(8).align_y(Alignment::Center);
    name_line = name_line.push(if o.verified {
        container(row![icon(icons::circle_check(), 12, hex(0x93c5fd)), text("Verified").size(11).font(BOLD).color(hex(0x93c5fd))].spacing(4).align_y(Alignment::Center)).padding([2, 7]).style(badge(rgba(0x3b82f6, 0.15), rgba(0x3b82f6, 0.3)))
    } else {
        container(text("Not verified").size(11).color(MUTED)).padding([2, 7]).style(badge(rgba(0xffffff, 0.05), rgba(0xffffff, 0.0)))
    });
    let mut about = column![name_line].spacing(4).width(Fill);
    if !o.description.is_empty() {
        about = about.push(text(o.description.as_str()).size(12.5).color(TEXT_BODY));
    }
    if let Some(w) = o.website.as_deref().filter(|w| !w.is_empty()) {
        about = about.push(text(w).size(12).color(INDIGO_SOFT));
    }
    let face: El = if owner { logo_picker(&o.name, o.logo.as_deref(), Some(o.id), 60.0) } else { avatar_pic(&o.name, o.logo.as_deref(), 60.0, None) };
    let top = row![face, about, button(icon(icons::x(), 16, MUTED)).padding(2).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(m(OrgMsg::Close))].spacing(14).align_y(Alignment::Start);

    let full = o.max_members > 0 && o.members.len() as i64 >= o.max_members;
    let mut people: Column<'a, Message> = column![].spacing(6);
    for p in &o.members {
        let name = p.display_name.as_deref().filter(|n| !n.is_empty()).unwrap_or(&p.username);
        let mut line = row![
            avatar_pic(name, p.avatar.as_deref(), 28.0, None),
            column![text(name).size(13).font(SEMIBOLD).color(Color::WHITE), text(format!("@{}", p.username)).size(11).color(TEXT_SECONDARY)].spacing(1).width(Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if !p.title.is_empty() {
            line = line.push(text(p.title.as_str()).size(12).color(TEXT_BODY));
        }
        if p.owner {
            line = line.push(container(text("OWNER").size(10).font(BOLD).color(AMBER_TEXT)).padding([1, 6]).style(badge(rgba(0xf59e0b, 0.12), rgba(0xf59e0b, 0.3))));
        } else if owner {
            line = line.push(button(icon(icons::x(), 12, MUTED)).padding(4).style(ghost(5.0, rgba(0xef4444, 0.1))).on_press_maybe((!busy).then(|| m(OrgMsg::Remove(o.id, p.user_id)))));
        }
        people = people.push(container(line).padding([6, 10]).style(crate::theme::soft_row));
    }
    let count = text(format!("PEOPLE · {} / {}", o.members.len(), o.max_members)).size(10.5).font(BOLD).color(if full { AMBER_TEXT } else { FAINT });
    let mut col = column![top, count, scrollable(people).height(iced::Length::Shrink).style(thin_scroll)].spacing(14);
    if owner {
        col = col.push(if full {
            El::from(text("It's full — ask Accordo for more room.").size(11.5).color(AMBER_TEXT))
        } else {
            let id = o.id;
            row![
                text_input("username", who).on_input(|v| m(OrgMsg::AddName(v))).on_submit(m(OrgMsg::Add(id))).size(13).padding([8, 10]).style(field).width(Fill),
                text_input("title (optional)", title).on_input(|v| m(OrgMsg::AddTitle(v))).on_submit(m(OrgMsg::Add(id))).size(13).padding([8, 10]).style(field).width(150),
                btn("Add", Tone::Accent, (!busy).then_some(OrgMsg::Add(id))),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        });
    }
    col = col.push(row![
        if owner { btn("Delete organization", Tone::Danger, (!busy).then_some(OrgMsg::Delete(o.id))) } else { btn("Leave", Tone::Dark, (!busy).then_some(OrgMsg::Remove(o.id, me))) },
        space().width(Fill),
        btn("Close", Tone::Dark, Some(OrgMsg::Close)),
    ]
    .spacing(8));
    col.into()
}

pub fn view<'a>(w: &'a Window, st: &'a Orgs, me: i64) -> El<'a> {
    let body = match w {
        Window::New(f) => new_org(f, st),
        Window::Org(id, who, title) => match st.mine.as_ref().and_then(|x| x.orgs.iter().find(|o| o.id == *id)) {
            Some(o) => org_window(o, who, title, st, me),
            None => column![header("Organization", "It's gone, or you're no longer in it".into())].into(),
        },
    };
    overlay::modal(container(body).width(560).max_height(640).padding(22).style(modal), m(OrgMsg::Close))
}
