// SPDX-License-Identifier: GPL-3.0-or-later
//! Client-side state shared across screens (the web app's src/store/, zustand):
//!
//! | here        | web                          |
//! |-------------|------------------------------|
//! | app_store   | store/useAppStore.ts         |
//! | rooms_store | store/useRoomsStore.ts       |
//!
//! Plain data with methods — iced keeps it in the `App`. What the web store did as a side effect
//! (saving wallets to the account, patching user data) comes back as a `StoreEffect` for the app
//! to run with the API. Ported ahead of the screens that will use it, hence the allow below.

#![allow(dead_code)]

pub mod app_store;
pub mod room_permissions;
pub mod rooms_store;
pub mod secrets;
pub mod unsaved;
