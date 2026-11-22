// SPDX-License-Identifier: GPL-3.0-or-later
//! The window frame: the messages of this area, handled by the app.
//!
//! The title bar's buttons, the window being closed or unfocused, a file dropped on it, the privacy
//! screen coming back, and where the focus goes.
//!
//! Kept apart from the handler that answers them so the types can be read in one place;
//! the handler is `app/windows.rs`.

use super::super::*;

#[derive(Debug, Clone)]
pub enum WindowMsg {
    /// The title bar's buttons (drag, minimize, maximize, close).
    Action(WindowAction),
    /// The window was closed (the main one quits the app).
    Closed(window::Id),
    /// Another window came in front (the privacy screen, if it's on).
    Blurred(window::Id),
    /// A file was dropped on a window: attach it.
    FileDropped(window::Id, std::path::PathBuf),
    /// A key while the privacy screen was up: show the app again.
    Unhide,
    /// Tab / Shift+Tab: the next (or previous) focusable.
    FocusNext(bool),
}
