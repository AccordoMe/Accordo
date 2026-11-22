// SPDX-License-Identifier: GPL-3.0-or-later
//! Toasts in the bottom-right corner (context/ToastContext.tsx): click or 5 s to dismiss.

use std::time::Duration;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, text};
use iced::{Fill, Task};

use super::ui::El;
use crate::app::Message;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Success: once actions (send a cheque, pay) report back
pub enum Kind {
    Info,
    Success,
    Error,
}

struct Toast {
    id: u64,
    kind: Kind,
    message: String,
}

#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
    next: u64,
}

impl Toasts {
    /// Shows a toast; the returned task dismisses it after 5 s.
    pub fn show(&mut self, kind: Kind, message: impl Into<String>) -> Task<Message> {
        let id = self.next;
        self.next += 1;
        let message = message.into();
        match kind {
            Kind::Error => tracing::warn!("toast: {message}"),
            _ => tracing::info!("toast: {message}"),
        }
        self.items.push(Toast { id, kind, message });
        Task::perform(tokio::time::sleep(Duration::from_secs(5)), move |_| Message::DismissToast(id))
    }

    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|t| t.id != id);
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn view(&self) -> El<'_> {
        let stack = column(self.items.iter().map(|t| {
            button(container(text(&t.message).size(14)).padding([12, 18]).max_width(360).style(theme::toast(t.kind)))
                .padding(0)
                .style(|_, _| button::Style::default())
                .on_press(Message::DismissToast(t.id))
                .into()
        }))
        .spacing(8)
        .align_x(Horizontal::Right);
        container(stack).width(Fill).height(Fill).padding(24).align_x(Horizontal::Right).align_y(Vertical::Bottom).into()
    }
}
