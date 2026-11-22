// SPDX-License-Identifier: GPL-3.0-or-later
//! The finance lists behind the Cheques and Invoices pages (`Lists`, `components/finance/records`):
//! loading a kind of list, refreshing it, the date & time window, and the chat's attach window
//! (cheques / invoices / campaigns / drops to send with a message).
//!
//! Responsible for: the list state in `App` and the requests that fill it. Not here: drawing the
//! rows (`components/finance/records.rs`), and putting a cheque or invoice into a message
//! (`pages/chat_media.rs`).

use super::*;

impl App {

    /// Loads one list of cheques / invoices (kept while it refreshes).
    pub(super) fn load_list(&mut self, k: crate::components::finance::records::ListKind) -> Task<Message> {
        use crate::components::finance::records::{ListKind, ListsMsg};
        let Some(api) = self.session.as_ref().filter(|s| !s.token.is_empty()).map(Session::api) else { return Task::none() };
        let list = self.lists.get_mut(k);
        if list.loading {
            return Task::none();
        }
        list.loading = true;
        let fut = async move {
            match k {
                ListKind::IncomingCheques => crate::api::cheques::incoming(api).await,
                ListKind::MyCheques => crate::api::cheques::mine(api).await,
                ListKind::IncomingInvoices => crate::api::invoices::incoming(api).await,
                ListKind::MyInvoices => crate::api::invoices::mine(api).await,
            }
            .map_err(|e| e.message)
        };
        Task::perform(fut, move |r| Message::Lists(ListsMsg::Loaded(k, r)))
    }

    /// The lists on screen that were never loaded: load them.
    pub(super) fn ensure_lists(&mut self) -> Task<Message> {
        use crate::components::finance::records::ListKind;
        use crate::pages::cheques_page::ChequesTab;
        use crate::pages::invoices_page::InvoicesTab;
        let mut want = Vec::new();
        match self.nav {
            Nav::Cheques => match self.cheques.tab {
                ChequesTab::Incoming => want.push(ListKind::IncomingCheques),
                ChequesTab::Outgoing => want.push(ListKind::MyCheques),
                ChequesTab::Write => {}
            },
            Nav::Invoices => match self.invoices.tab {
                InvoicesTab::Incoming => want.push(ListKind::IncomingInvoices),
                InvoicesTab::Yours => want.push(ListKind::MyInvoices),
                InvoicesTab::Issue => {}
            },
            _ => {}
        }
        if self.attach.is_some() {
            want.extend([ListKind::MyCheques, ListKind::MyInvoices]);
        }
        want.retain(|k| self.lists.get(*k).items.is_none() && self.lists.get(*k).error.is_none());
        let tasks: Vec<Task<Message>> = want.into_iter().map(|k| self.load_list(k)).collect();
        Task::batch(tasks)
    }

    pub(super) fn lists_update(&mut self, m: crate::components::finance::records::ListsMsg) -> Task<Message> {
        use crate::components::finance::records::ListsMsg;
        match m {
            ListsMsg::Load(k) => {
                self.lists.get_mut(k).error = None;
                self.load_list(k)
            }
            ListsMsg::Loaded(k, r) => {
                let list = self.lists.get_mut(k);
                list.loading = false;
                match r {
                    Ok(items) => {
                        list.items = Some(items);
                        list.error = None;
                    }
                    Err(e) => list.error = Some(e),
                }
                Task::none()
            }
            ListsMsg::Toggle(k, id) => {
                let list = self.lists.get_mut(k);
                if list.open.as_deref() == Some(id.as_str()) {
                    list.open = None;
                    return Task::none();
                }
                list.open = Some(id.clone());
                // The card's live copy (and claims / payments), as in the chat.
                let Some(api) = self.session.as_ref().map(Session::api) else { return Task::none() };
                if k.cheques() {
                    crate::pages::chat_media::reload_cheque(&mut self.chat, &api, &id)
                } else {
                    crate::pages::chat_media::load_invoice(&mut self.chat, &api, &id)
                }
            }
        }
    }

    /// The date & time window: what it gives back depends on who opened it.
    pub(super) fn time_update(&mut self, m: crate::components::time_picker::TimeMsg) -> Task<Message> {
        use crate::components::finance::duration_picker::Duration;
        use crate::components::time_picker::{Purpose, State, TimeMsg};
        match m {
            TimeMsg::Open(purpose, at) => {
                self.chat.panel = None;
                self.time_picker = Some(State::new(purpose, at));
                Task::none()
            }
            TimeMsg::Close => {
                self.time_picker = None;
                Task::none()
            }
            TimeMsg::Confirm => {
                let Some(st) = self.time_picker.as_ref() else { return Task::none() };
                if st.problem(chrono::Local::now().timestamp()).is_some() {
                    return Task::none();
                }
                let Some(at) = st.picked() else { return Task::none() };
                let (purpose, style) = (st.purpose, st.style);
                self.time_picker = None;
                match purpose {
                    Purpose::Chat => Task::done(Message::Chat(chat_page::ChatMsg::InsertTime(crate::components::chat::timestamp::token(at, style)))),
                    Purpose::ChequeExpiry => Task::done(Message::Cheques(ChequesMsg::Expiry(Duration::At(at)))),
                    Purpose::InvoiceDue => Task::done(Message::Invoices(InvoicesMsg::Due(Duration::At(at)))),
                    #[cfg(feature = "crowdfund")]
                    Purpose::CampaignEnd => Task::done(Message::Crowdfund(CrowdfundMsg::Duration(Duration::At(at)))),
                    // The drop form comes with drops; nothing takes it yet.
                    #[cfg(feature = "drops")]
                    Purpose::DropStart => Task::done(Message::Drops(crate::pages::drops_page::DropsMsg::Expiry(Duration::At(at)))),
                }
            }
            other => {
                if let Some(st) = self.time_picker.as_mut() {
                    st.update(other);
                }
                Task::none()
            }
        }
    }

    /// The chat's attach window: open, search, filter, pick.
    pub(super) fn attach_update(&mut self, m: crate::components::chat::attach_picker::AttachMsg) -> Task<Message> {
        use crate::components::chat::attach_picker::{AttachMsg, State};
        use crate::components::finance::records::ListKind;
        match m {
            AttachMsg::Open(tab) => {
                use crate::components::chat::attach_picker::TABS;
                // A hidden section's tab: the first one that's on (none on: nothing to attach).
                let modules = self.modules();
                let Some(tab) = std::iter::once(tab).chain(TABS).find(|t| modules.shown(t.module())) else { return Task::none() };
                self.chat.panel = None;
                self.attach = Some(State::new(tab));
                // Fresh lists every time it opens (shown at once if they're cached).
                return Task::batch([self.load_list(ListKind::MyCheques), self.load_list(ListKind::MyInvoices), self.load_shareables(), operation::focus(crate::components::chat::attach_picker::SEARCH)]);
            }
            AttachMsg::Close => self.attach = None,
            AttachMsg::Refresh => {
                let Some(a) = &self.attach else { return Task::none() };
                return match a.tab.list() {
                    Some(k) => {
                        self.lists.get_mut(k).error = None;
                        self.load_list(k)
                    }
                    None => self.load_shareables(),
                };
            }
            AttachMsg::Pick(kind, item) => {
                self.attach = None;
                return Task::done(Message::Chat(chat_page::ChatMsg::AttachFinance(kind, item)));
            }
            other => {
                if let Some(a) = self.attach.as_mut() {
                    a.update(other);
                }
            }
        }
        Task::none()
    }

    /// Campaigns (yours + live ones) and your open drops for the attach window.
    pub(super) fn load_shareables(&mut self) -> Task<Message> {
        use crate::components::chat::attach_picker::AttachMsg;
        let Some(api) = self.session.as_ref().filter(|s| !s.token.is_empty()).map(Session::api) else { return Task::none() };
        let a2 = api.clone();
        let campaigns = async move {
            let (mine, live) = iced::futures::join!(crate::api::crowdfund::get_my(a2.clone()), crate::api::crowdfund::active(a2));
            if let (Err(e), Err(_)) = (&mine, &live) {
                return Err(e.message.clone());
            }
            let mut out = mine.map(|m| m.created).unwrap_or_default();
            for c in live.unwrap_or_default() {
                if !out.iter().any(|x| x.campaign_id == c.campaign_id) {
                    out.push(c);
                }
            }
            Ok(out)
        };
        Task::batch([
            Task::perform(campaigns, |r| Message::Attach(AttachMsg::Campaigns(r))),
            Task::perform(crate::api::drops::mine(api), |r| Message::Attach(AttachMsg::Drops(r.map_err(|e| e.message)))),
        ])
    }
}
