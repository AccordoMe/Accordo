// SPDX-License-Identifier: GPL-3.0-or-later
//! Accordo — native desktop client (prototype). Pure Rust UI on iced: no WebView.
//! Talks to the real API: sign-in (remembered in the OS credential store), the chat (live over
//! the socket), settings.

// The API client and the E2EE primitives live in the shared core (`../accordo-core`), so the same
// code drives the desktop client and the Android app. Re-exported under the old name: every
// `crate::api::…` path in this crate keeps working.
pub use accordo_core::api;

mod app;
mod avatars;
mod chain_rpc;
mod components;
#[cfg(feature = "dev-screenshot")]
mod dev;
mod devtools;
mod e2ee;
mod evm;
mod flows;
mod hotkeys;
mod icons;
mod media;
mod module_host;
mod modules;
mod wallets;
mod pages;
mod player;
mod recorder;
mod store;
mod theme;
mod usage;

use iced::{window, Size};

/// The app's id on Linux: Wayland's app_id / X11's WM_CLASS. Matches the desktop entry
/// (assets/linux/me.accordo.Accordo.desktop), so the panel shows the Accordo icon and groups windows.
pub const APP_ID: &str = "me.accordo.Accordo";

/// The window icon (the panel's, Alt+Tab's): the logo, decoded once.
pub fn window_icon() -> Option<window::Icon> {
    let img = image::load_from_memory_with_format(include_bytes!("../assets/accordo-icon-192.png"), image::ImageFormat::Png).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    window::icon::from_rgba(img.into_raw(), w, h).ok()
}

/// `ACCORDO_TILE=left|right` (scripts/dev-two.sh): the main window takes that half of its screen,
/// so two test windows sit side by side. Wayland doesn't let an app place its own window — the
/// script runs them through XWayland for this; elsewhere the size still applies.
fn tile(id: window::Id) -> iced::Task<app::Message> {
    let side = match std::env::var("ACCORDO_TILE").as_deref() {
        Ok("left") => 0.0,
        Ok("right") => 1.0,
        _ => return iced::Task::none(),
    };
    window::monitor_size(id).then(move |screen| {
        let Some(screen) = screen else { return iced::Task::none() };
        let width = (screen.width / 2.0).max(960.0);
        let x = side * (screen.width - width).max(0.0);
        iced::Task::batch([window::resize(id, Size::new(width, screen.height)), window::move_to(id, iced::Point::new(x, 0.0))])
    })
}

/// Window settings every window of the app shares: its id and icon.
pub fn window_base() -> window::Settings {
    window::Settings {
        icon: window_icon(),
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific { application_id: APP_ID.into(), ..Default::default() },
        ..Default::default()
    }
}

use app::App;

pub fn main() -> iced::Result {
    devtools::log_layer::init();
    // The core reports its requests and socket frames into this panel (logcat on a phone).
    devtools::install();
    // Sessions and chat keys go to the OS credential store (the phone uses its Keystore).
    store::secrets::install();
    // UI scale, Developer mode, the accounts on this computer… (the web keeps the same in localStorage).
    let store = store::app_store::AppStore::load();
    // Read before the window opens: the open account's remembered sign-in goes straight to the app.
    let previous = store.active_account.as_deref().and_then(|k| store.account(k)).map(|a| a.previous_key());
    let saved = api::session_store::load(store.active_account.as_deref(), previous.as_deref());
    // Screenshot runs with a demo user don't touch the real saved session.
    #[cfg(feature = "dev-screenshot")]
    let saved = saved.filter(|_| std::env::var_os("ACCORDO_DEMO_USER").is_none());
    let size = Size::new(1400.0, 880.0);
    // Screenshots of long pages: ACCORDO_SIZE=1400x2000.
    #[cfg(feature = "dev-screenshot")]
    let size = std::env::var("ACCORDO_SIZE")
        .ok()
        .and_then(|v| v.split_once('x').and_then(|(w, h)| Some(Size::new(w.parse().ok()?, h.parse().ok()?))))
        .unwrap_or(size);
    // A daemon, not a one-window application: the developer tools open in a window of their own
    // (F12). The main window is opened here; closing it quits (App::window_closed).
    let main_window = window::Settings {
        size,
        min_size: Some(Size::new(960.0, 600.0)),
        position: window::Position::Centered,
        // Frameless, like the Tauri shell: the dark top bar is the title bar.
        decorations: false,
        ..window_base()
    };
    let mut application = iced::daemon(
        move || {
            let (mut app, boot) = App::new(saved.clone(), store.clone());
            let (id, open) = window::open(main_window.clone());
            app.main_window = Some(id);
            (app, iced::Task::batch([open.then(tile), boot]))
        },
        // After every update: load the avatars the views asked for.
        |app: &mut App, m| {
            let task = app.update(m);
            let api = app.session.as_ref().map(crate::api::auth::Session::api);
            iced::Task::batch([task, avatars::pump(api.as_ref())])
        },
        App::view_window,
    )
    .scale_factor(|app: &App, _| app.store.ui_scale.clamp(0.5, 3.0))
    .title(App::title)
    .subscription(App::subscription)
    .default_font(theme::INTER)
    .style(|_, _| iced::theme::Style { background_color: theme::BG_DARK, text_color: theme::TEXT });
    for font in theme::FONTS {
        application = application.font(font);
    }
    application.run()
}
