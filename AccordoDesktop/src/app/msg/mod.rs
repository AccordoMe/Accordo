// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's message types — what the UI and the subscriptions tell `App` happened, one file per
//! area. `update` matches the outer `Message` and hands each area enum to that area's handler
//! (`session_update`, `window_update`, `data_update`), which lives with the code it changes.
//!
//! Adding a group: define the enum here, add the variant `Message::<Area>(<Area>Msg)` in
//! `app/mod.rs`, one dispatcher arm in `update`, and rewrite the construction sites
//! (`Message::X(a)` → `Message::<Area>(<Area>Msg::X(a))`). The full recipe, with the two traps,
//! is in `docs/app-modules.md`.

pub mod data;
pub mod session;
pub mod window;

pub use data::DataMsg;
pub use session::SessionMsg;
pub use window::WindowMsg;
