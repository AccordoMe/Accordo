// SPDX-License-Identifier: GPL-3.0-or-later
//! Page frames (Layout.tsx): the app shell after sign-in, and the bare one for auth screens.

use iced::widget::{column, row, space, stack};
use iced::Fill;

use super::ui::{hline, vline, El};
use super::{desktop, key_backup_modal, sidebar, top_bar, wallet_panel};
use crate::api::auth::Session;
use crate::app::{App, Nav, DataMsg};
use crate::pages::{chat_page, cheques_page, invoices_page, settings_page};
#[cfg(feature = "crowdfund")]
use crate::pages::crowdfund_page;
#[cfg(feature = "drops")]
use crate::pages::drops_page;

/// Top bar + icon rail + the current page.
pub fn app<'a>(app: &'a App, session: &'a Session) -> El<'a> {
    let page: El = match app.nav {
        Nav::Chat => chat_page::view(&app.chat),
        Nav::Cheques => cheques_page::view(&app.cheques, app.token_field(app.cheques.token.as_ref()), app.token_field(app.cheques.get_token.as_ref()), &app.lists, &session.user.username, cards(app)),
        Nav::Invoices => invoices_page::view(&app.invoices, app.token_field(app.invoices.token.as_ref()), &app.lists, &session.user.username, cards(app)),
        #[cfg(feature = "drops")]
        Nav::Drops => drops_page::view(&app.drops, &app.store.drop_plans, app.token_field(app.drops.form.token.as_ref()), &app.drops.web_base),
        #[cfg(feature = "crowdfund")]
        Nav::Crowdfund => crowdfund_page::view(&app.crowdfund, app.token_field(app.crowdfund.token.as_ref()), app.native_symbol(), &session.user.username),
        Nav::Settings => {
            settings_page::view(&session.user, session.user.limits(&app.chat.rooms.rooms), &app.settings, &app.chat_key, vault_status(app))
        }
        // A section this build doesn't have (Navigate never opens one).
        #[cfg(not(all(feature = "drops", feature = "crowdfund")))]
        _ => chat_page::view(&app.chat),
    };
    // Records live on-chain but not in Accordo: over every page but the chat (the web hides it
    // there too; it still retries quietly).
    // The page keeps its place in the tree whether the banner / the rail is shown or not (an
    // empty space stands in), so its scroll and fields aren't started over (see App::view).
    let banner: El = if app.unsaved.is_empty() || app.nav == Nav::Chat { space().height(0).into() } else { unsaved_banner(&app.unsaved, app.unsaved_retry == Some(true)) };
    let page: El = column![banner, page].into();
    let mut body = row![];
    if app.rail_hidden {
        body = body.push(space().width(0)).push(space().width(0));
    } else {
        let open = match &app.orgs.window {
            Some(crate::app::orgs::Window::Org(id, ..)) => Some(*id),
            _ => None,
        };
        let orgs = sidebar::Orgs { list: app.orgs.mine.as_ref().map(|m| m.orgs.as_slice()).unwrap_or(&[]), open, can_create: app.orgs.mine.as_ref().is_some_and(|m| m.can_create) };
        body = body.push(sidebar::view(app.nav, app.chat.total_unread(), app.modules(), session.user.is_admin, app.admin.open, orgs)).push(vline());
    }
    let shell = column![
        top_bar::view(&session.user, app.profile_open, &app.chains, app.selected_chain(), app.active_wallet()),
        hline(),
        body.push(page).height(Fill)
    ];
    // Layers over the page, bottom to top: profile popover, wallet drawer, modal.
    let mut layers = stack![shell];
    if app.profile_open {
        let me = session.key();
        let others = app.store.accounts_by_use().into_iter().filter(|a| a.key() != me).collect();
        let base = crate::api::client::normalize_base(&session.base);
        let server = base.split_once("://").map_or(base.clone(), |(_, h)| h.to_string());
        layers = layers.push(top_bar::profile_menu(&session.user, server, others));
    }
    if app.wallet_open {
        use crate::components::settings::master_vault;
        let status = vault_status(app);
        let ctx = wallet_panel::Ctx {
            wallets: &app.store.wallets,
            active: app.store.active_wallet_id.as_deref(),
            has_password: status.has_password,
            unlocked: status.unlocked,
            lock_status: status.unlocked.then(|| master_vault::lock_badge(status)),
            unlock_form: master_vault::unlock_form(&app.settings.vault),
            api_key_session: session.user.session_via_api_key,
            public: session.user.public_wallets.as_deref(),
        };
        layers = layers.push(wallet_panel::view(&app.wallet_ui, ctx));
        let find = |id: &str| app.store.wallets.iter().find(|w| w.id == id);
        if let Some(w) = app.wallet_ui.delete.as_deref().and_then(find) {
            layers = layers.push(wallet_panel::delete_view(w));
        }
        if let Some((r, w)) = app.wallet_ui.reveal.as_ref().and_then(|r| find(&r.wallet_id).map(|w| (r, w))) {
            layers = layers.push(wallet_panel::reveal_view(r, w));
        }
    }
    if let Some(state) = &app.key_backup {
        layers = layers.push(key_backup_modal::view(state));
    }
    if let Some(w) = &app.orgs.window {
        layers = layers.push(super::org_modal::view(w, &app.orgs, session.user.id));
    }
    if let Some(picker) = &app.picker {
        layers = layers.push(super::file_picker::view(picker));
    }
    // A module's request waiting on the user: its window over everything.
    let mc = &app.module_console;
    if let (Some(host), Some(wait)) = (&mc.host, mc.front()) {
        if let Some(w) = super::module_window::view(host, wait, &mc.window) {
            layers = layers.push(w);
        }
    }
    layers.into()
}

/// UnsavedRecordsBanner.tsx: what's waiting, Backup and Retry now.
fn unsaved_banner<'a>(kinds: &[String], busy: bool) -> El<'a> {
    use crate::app::Message;
    use crate::components::ui::icon;
    use crate::icons;
    use crate::theme::*;
    use iced::widget::{button, container, text};
    use iced::{Alignment, Color};
    let one = kinds.len() == 1;
    let line = iced::widget::rich_text![
        iced::widget::span::<(), iced::Font>(format!("{} live on-chain but not saved in Accordo yet. ", crate::store::unsaved::summary(kinds))).font(SEMIBOLD).color(Color::WHITE),
        iced::widget::span(format!("{} kept on this computer — don't delete the app's data until {} saved.", if one { "It is" } else { "They are" }, if one { "it's" } else { "they're" })).color(TEXT_SECONDARY),
    ]
    .size(13);
    let backup = button(row![icon(icons::download(), 13, Color::WHITE), text("Backup").size(12).font(SEMIBOLD)].spacing(5).align_y(Alignment::Center))
        .padding([7, 12])
        .style(tone_button(Tone::Dark))
        .on_press(Message::Data(DataMsg::UnsavedBackup));
    let retry = button(row![icon(icons::refresh_cw(), 13, Color::WHITE), text(if busy { "Saving…" } else { "Retry now" }).size(12).font(SEMIBOLD)].spacing(5).align_y(Alignment::Center))
        .padding([7, 14])
        .style(tone_button(Tone::Accent))
        .on_press_maybe((!busy).then_some(Message::Data(DataMsg::UnsavedRetry)));
    container(
        container(row![icon(icons::triangle_alert(), 18, AMBER_TEXT), container(line).width(iced::Fill), backup, retry].spacing(12).align_y(Alignment::Center))
            .padding([10, 14])
            .style(boxed(rgba(0xf59e0b, 0.08), rgba(0xf59e0b, 0.35), 10.0)),
    )
    .padding([12, 16])
    .into()
}

/// The chat's cards for the lists' open rows (records::list_view).
fn cards(app: &App) -> crate::components::finance::records::Cards<'_> {
    crate::components::finance::records::Cards { cheques: &app.chat.cheques, invoices: &app.chat.invoices, explorers: &app.chat.explorers, wallets: &app.chat.my_wallets, me: &app.chat.me_name }
}

/// Sign-in and similar screens: just the slim title bar above the page.
pub fn auth<'a>(page: El<'a>) -> El<'a> {
    column![desktop::fallback_title_bar(), page].into()
}

/// The wallet session as the settings card and the wallet drawer show it.
fn vault_status(app: &App) -> crate::components::settings::master_vault::Status {
    let has_sealed = app.store.wallets.iter().any(|w| w.vault.parsed().is_some());
    crate::components::settings::master_vault::Status {
        unlocked: app.wallet_session.unlocked(),
        lock_at: app.wallet_session.lock_at,
        now_ms: chrono::Utc::now().timestamp_millis(),
        has_password: app.store.encrypted_vault.is_some() || has_sealed,
        has_sealed,
    }
}
