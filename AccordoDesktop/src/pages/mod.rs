// SPDX-License-Identifier: GPL-3.0-or-later
//! Screens (the web app's src/pages/).

pub mod admin_page;
pub mod auth_forms;
pub mod chat_actions;
pub mod chat_media;
pub mod chat_page;
pub mod chat_polls;
pub mod cheques_page;
#[cfg(feature = "crowdfund")]
pub mod crowdfund_page;
#[cfg(feature = "drops")]
pub mod drops_page;
pub mod invoices_page;
pub mod login;
pub mod room_admin;
pub mod settings_page;
pub mod splash;

#[cfg_attr(not(feature = "drops"), allow(dead_code))] // Run, Info, Batch: sent only by the drops and campaign pages
/// What a page asks the app to do after handling one of its messages.
pub enum Effect {
    None,
    /// The control needs something the desktop client doesn't have yet (wallet vault, contracts).
    NotYet(&'static str),
    /// Open a page of the web app in the browser.
    OpenWeb(&'static str),
    /// A message for the person (the form isn't right).
    Error(String),
    /// Open the date & time window; what it picks comes back to the page as a message.
    PickTime(crate::components::time_picker::Purpose, Option<i64>),
    /// Run this (the page's own loading).
    Run(iced::Task<crate::app::Message>),
    /// A note for the person (done / added).
    Info(String),
    /// Copy this to the clipboard ("Link" copied).
    Copy(String, &'static str),
    Batch(Vec<Effect>),
    /// The drop form is filled in: review it, then sign.
    #[cfg(feature = "drops")]
    CreateDrop(Box<crate::flows::drops::Draft>),
    /// Take back what nobody claimed.
    #[cfg(feature = "drops")]
    RefundDrop(Box<crate::api::drops::DropInfo>),
    /// The campaign form is filled in.
    #[cfg(feature = "crowdfund")]
    LaunchCampaign(Box<crate::flows::crowdfund::LaunchDraft>),
    /// Back a campaign (looked up by its id first).
    #[cfg(feature = "crowdfund")]
    BackCampaign(Box<crate::flows::crowdfund::BackDraft>),
    /// Withdraw / end early / refund everyone / get your refund.
    #[cfg(feature = "crowdfund")]
    CampaignAction(Box<crate::api::crowdfund::CampaignRead>, crate::flows::crowdfund::Action),
    /// 25 % … MAX: read the wallet's balance of the campaign's token, then fill the amount in.
    #[cfg(feature = "crowdfund")]
    ContributionShare(u8),
    /// The invoice form is filled in: review it, then sign.
    IssueInvoice(Box<crate::flows::invoices::Draft>),
    /// The cheque form is filled in: review it, then sign.
    WriteCheque(Box<cheques_page::Draft>),
    /// An @ was typed in an address field: open the contacts for it.
    Contacts(crate::components::contact_picker::Target),
}
