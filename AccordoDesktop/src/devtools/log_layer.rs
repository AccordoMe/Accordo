// SPDX-License-Identifier: GPL-3.0-or-later
//! Logging: `tracing` events go to the Console tab and, filtered by RUST_LOG, to stderr.

use std::fmt::Write;

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Our crate's events (debug and up) into the Console tab.
struct ConsoleLayer;

#[derive(Default)]
struct Message(String);

impl Visit for Message {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            let _ = write!(self.0, " {}={value}", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }
}

impl<S: Subscriber> Layer<S> for ConsoleLayer {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        if !super::enabled() || !meta.target().starts_with("accordo") || *meta.level() > tracing::Level::DEBUG {
            return;
        }
        let mut message = Message::default();
        event.record(&mut message);
        super::record_log(*meta.level(), meta.target(), message.0);
    }
}

/// Call once at start-up. stderr shows what RUST_LOG asks for (default: warnings and errors,
/// without the window library's harmless "XDG Settings Portal didn't answer" on Wayland).
pub fn init() {
    let stderr = tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,sctk_adwaita=off")));
    let registry = tracing_subscriber::registry().with(stderr);
    // Always installed: Developer mode can switch recording on later (it checks enabled() itself).
    let _ = registry.with(ConsoleLayer).try_init();
}
