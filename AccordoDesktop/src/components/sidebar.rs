// SPDX-License-Identifier: GPL-3.0-or-later
//! The icon rail on the left (Sidebar.tsx, always collapsed on desktop).

use iced::widget::{button, center, column, container, scrollable, space, stack, svg, text, tooltip};
use iced::alignment::Horizontal;
use iced::{Color, Fill};

use super::ui::{dot_badge, icon, El};
use crate::app::{Message, Nav};
use crate::icons;
use crate::theme::*;

/// The rail's organizations: the ones I'm in, and which one's window is open.
pub struct Orgs<'a> {
    pub list: &'a [crate::api::orgs::OrgCard],
    pub open: Option<i64>,
    /// May create one (Enterprise, or an admin): only then the "+".
    pub can_create: bool,
}

fn hint<'a>(content: impl Into<El<'a>>, label: String) -> El<'a> {
    tooltip(content, container(text(label).size(11.5).color(TEXT_BODY)).padding([4, 8]).style(boxed(hex(0x1c1d22), BORDER, 6.0)), tooltip::Position::Right).into()
}

/// `admin`: the account is an admin (the shield opens the Admin Console; `admin_open` lights it).
/// `modules`: the sections the user hid (or the server closed) aren't on the rail.
pub fn view<'a>(current: Nav, chat_unread: u32, modules: crate::modules::Modules, admin: bool, admin_open: bool, orgs: Orgs<'a>) -> El<'a> {
    let item = |handle: svg::Handle, nav: Nav, badge: u32| -> El<'a> {
        let active = current == nav;
        let glyph = icon(handle, 18, if active { Color::WHITE } else { TEXT_SECONDARY });
        let content: El = if badge > 0 {
            stack![
                center(glyph),
                container(dot_badge(badge)).width(Fill).height(Fill).align_x(Horizontal::Right).padding([2, 4]),
            ]
            .into()
        } else {
            center(glyph).into()
        };
        button(content)
            .width(52)
            .height(42)
            .padding(0)
            .style(selectable(active, ACCENT, 8.0))
            .on_press(Message::Navigate(nav))
            .into()
    };

    let mut col = column![item(icons::message_square(), Nav::Chat, chat_unread)].spacing(4);
    for (handle, nav) in [(icons::wallet(), Nav::Cheques), (icons::receipt(), Nav::Invoices), (icons::send(), Nav::Drops), (icons::flame(), Nav::Crowdfund)] {
        if modules.page_shown(nav) {
            col = col.push(item(handle, nav, 0));
        }
    }
    // Under the menu: a line, my organizations, and "+" for a new one.
    let mut list = column![].spacing(6).align_x(Horizontal::Center);
    for o in orgs.list {
        let open = orgs.open == Some(o.id);
        let face = button(crate::components::ui::avatar_pic(&o.name, o.logo.as_deref(), 36.0, None))
            .width(44)
            .height(44)
            .padding(4)
            .style(selectable(open, ACCENT, 12.0))
            .on_press(Message::Org(crate::app::OrgMsg::Open(o.id)));
        list = list.push(hint(face, if o.verified { format!("{} ✓", o.name) } else { o.name.clone() }));
    }
    if orgs.can_create {
        let plus = button(center(icon(icons::plus(), 18, GREEN)))
            .width(44)
            .height(44)
            .padding(0)
            .style(ghost(22.0, rgba(0x10b981, 0.15)))
            .on_press(Message::Org(crate::app::OrgMsg::New));
        list = list.push(hint(plus, "New organization".into()));
    }
    // The line only when something's under it.
    if orgs.can_create || !orgs.list.is_empty() {
        col = col.push(container(container(space().width(32).height(1)).style(crate::theme::rule)).width(Fill).center_x(Fill).padding([6, 0]));
    }
    col = col.push(scrollable(container(list).width(Fill).center_x(Fill)).height(Fill).direction(scrollable::Direction::Vertical(scrollable::Scrollbar::new().width(0).scroller_width(0))));
    if admin {
        col = col.push(
            button(center(icon(icons::shield(), 18, if admin_open { Color::WHITE } else { TEXT_SECONDARY })))
                .width(52)
                .height(42)
                .padding(0)
                .style(selectable(admin_open, ACCENT, 8.0))
                .on_press(Message::Admin(crate::pages::admin_page::AdminMsg::Open)),
        );
    }
    col = col.push(item(icons::settings(), Nav::Settings, 0));
    container(col)
    .width(68)
    .height(Fill)
    .padding([16, 8])
    .style(panel(RAIL))
    .into()
}
