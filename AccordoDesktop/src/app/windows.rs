// SPDX-License-Identifier: GPL-3.0-or-later
//! The window frame: the title bar's buttons (drag, minimize, maximize, close), the window's own
//! events (closed, unfocused), a file dropped on it, the privacy screen coming back, and where the
//! keyboard focus goes next.
//!
//! Responsible for: what happens to the windows (the main one and the developer tools') — the
//! messages themselves are `msg/window.rs`. Not here: the window's layout (`components/layout.rs`, `desktop.rs`), what a key press
//! does (`keys.rs`), or the privacy screen's state, which is the session's (`session.rs`).

use super::*;

impl App {

    /// The messages of this area — see [`WindowMsg`].
    pub(super) fn window_update(&mut self, m: WindowMsg) -> Task<Message> {
        match m {
            WindowMsg::Action(action) => {
                let Some(id) = self.main_window else { return Task::none() };
                return match action {
                    WindowAction::Drag => window::drag(id),
                    WindowAction::ToggleMaximize => window::toggle_maximize(id),
                    WindowAction::Minimize => window::minimize(id, true),
                    WindowAction::Close => window::close(id),
                };
            }
            WindowMsg::FileDropped(id, path) => {
                if Some(id) == self.main_window {
                    return Task::done(Message::Chat(chat_page::ChatMsg::AttachPath(path)));
                }
            }
            WindowMsg::Closed(id) => {
                if Some(id) == self.main_window {
                    return iced::exit();
                }
                if self.devtools.window == Some(id) {
                    self.devtools.window = None;
                    // Docking: the tools stay open, under the app.
                    self.devtools.open = std::mem::take(&mut self.devtools.docking);
                    self.store.devtools_open = self.devtools.open;
                    self.store.devtools_filter = self.devtools.filter.clone();
                    return self.save_store();
                }
            }
            WindowMsg::Blurred(id) => {
                // Going to the developer tools isn't stepping away.
                if self.store.hide_on_blur && Some(id) == self.main_window && self.devtools.window.is_none() {
                    self.hide();
                }
            }
            WindowMsg::Unhide => {
                self.hidden = false;
                self.hidden_locked = false;
            }
            WindowMsg::FocusNext(back) => return if back { operation::focus_previous() } else { operation::focus_next() },
        }
        Task::none()
    }
}
