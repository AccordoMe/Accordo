// SPDX-License-Identifier: GPL-3.0-or-later
//! Sessions and accounts: the messages of this area, handled by the app.
//!
//! The profile menu, signing out, switching between the accounts
//! that stay signed in, the chat key arriving or failing.
//!
//! Kept apart from the handler that answers them so the types can be read in one place;
//! the handler is `app/session.rs`.

use super::super::*;

#[derive(Debug, Clone)]
pub enum SessionMsg {
    /// The profile menu in the top bar.
    ToggleMenu,
    Logout,
    SwitchTo(String),
    /// A saved account was read from the credential store — or isn't there any more.
    Loaded(String, Option<Saved>),
    AddAccount,
    Remove(String),
    /// The saved session was checked with its server.
    Restored(Result<Box<Session>, ApiError>),
    Saved(Result<(), String>),
    ChatKeyLoaded(Result<Option<DeviceKeypair>, String>),
    ChatKeyInstalled(Result<DeviceKeypair, String>),
    /// Every few minutes while the chat key has a lifetime in days: has it run out?
    ChatKeyClock,
    /// The lifetime was applied to this account's key: Ok(true) — it ran out and was deleted here.
    ChatKeyExpired(String, Result<bool, String>),
    /// Storage's "Delete chat key" reached the secret store (or why not).
    ChatKeyForgotten(Result<(), String>),
}
