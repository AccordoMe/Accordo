# Accordo — native desktop client (prototype)

Pure Rust UI on [iced](https://iced.rs) 0.14, no WebView. It mirrors the web app's look
(colors, sizes and icons are taken from `src/index.css`, `TopBar.tsx`, `Sidebar.tsx`,
`chat/ChatSidebar.tsx`, `LoginPage.tsx`). Sign-in talks to the real API
(`POST /api/auth/login`, 2FA included); the chat is live (rooms, messages, sending, the socket) —
public rooms fully; DMs and groups are end-to-end encrypted and wait for the E2EE port.
The session is remembered in the OS credential store (Secret Service / GNOME Keyring /
KWallet on Linux, Credential Manager on Windows, Keychain on macOS) and checked with
`GET /api/auth/me` at start-up; Logout forgets it. Several accounts (on one or several servers) can
stay signed in: the profile menu switches between them and adds one. Each token has its own entry
(`session:<id>:<nick>@<server>`, e.g. `session:1:dazay@https://accordo.me`; the chat key likewise `e2ee:…`); the list of accounts, which isn't secret, is in `state.json`. Logout signs
out of the open account only and opens the next one. The server defaults to `https://accordo.me`; change it on the sign-in screen or with
`ACCORDO_API_URL=http://127.0.0.1:8000`.

## Building

Rust **1.95 or newer**. The `rustc`/`cargo` from the distro's packages (apt on Ubuntu/Kubuntu) is
usually older and the build fails — use [rustup](https://rustup.rs) instead:

```sh
sudo apt remove --purge cargo rustc                              # if they were installed from apt
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh    # choose 1 (default install)
source "$HOME/.cargo/env"                                        # or open a new terminal
cargo build --release                                            # → target/release/accordo-native
```

Nothing else is needed on a desktop Linux (checked on Kubuntu, live session). If the link step
says `linker cc not found`: `sudo apt install build-essential`. `rustup update` keeps the compiler
current.

Voice messages and video notes — playing them and recording them — go through the system's
GStreamer, not a linked library: `gstreamer1.0-tools` plus `gstreamer1.0-plugins-{base,good,libav,ugly}`
(the first for `gst-launch-1.0`, libav for the AAC encoder, ugly for `x264enc`). Without it the app
runs, but ▶ opens the file in the system's player and the mic and camera say what's missing.

## Two windows for testing

`scripts/dev-two.sh` builds once and opens two windows side by side (`ACCORDO_TILE=left|right`:
half the screen each; on Wayland the script runs them through XWayland, since Wayland doesn't let
an app place its window); sign in as a different user in each to
try chats, cheques and invoices between them (`--release` for a release build, Ctrl+C closes
both). Each window has its own settings and accounts: the variable `ACCORDO_PROFILE=<name>` puts
them in `~/.config/accordo-desktop-<name>` (the first window uses your usual ones; names with
`A=… B=…`). The profile shows next to the logo and in the window title. The OS secret store is
shared — its entries are per account — so an account already signed in on this computer opens
with its chat key in either window.

## Installing as an app (Linux)

```sh
cargo build --release
scripts/install-linux.sh            # menu entry with the icon, for you (~/.local, no sudo)
scripts/install-linux.sh --system   # for everyone on the computer (/usr/local)
scripts/install-linux.sh --link     # development: the menu always starts the latest `cargo build --release`
scripts/install-linux.sh --uninstall
```

To give it to someone without Rust: `scripts/package-linux.sh` makes
`dist/accordo-<version>-linux-x86_64.tar.gz` (the binary, icon, menu entry, `install.sh`, licences);
they unpack it and run `./install.sh`. The binary needs glibc as new as the one it was built on
(printed in the archive's README.txt). The window carries the app id `me.accordo.Accordo`, so the
panel shows the Accordo icon (`assets/linux/`).

## Running

```sh
cargo run                    # debug
cargo run --release          # optimized (≈16 MB binary, fonts and icons included)
ICED_BACKEND=tiny-skia cargo run --release   # software rendering: ~30 MB RAM instead of ~150 MB with the GPU
```

Screenshot without a screen (checks the look): the window saves itself as a PNG and quits.

```sh
ACCORDO_SCREENSHOT=/tmp/login.png cargo run --features dev-screenshot
ACCORDO_SCREENSHOT=/tmp/chat.png ACCORDO_DEMO_USER=zuko ACCORDO_OPEN=2 cargo run --features dev-screenshot
ACCORDO_SCREENSHOT=/tmp/settings.png ACCORDO_DEMO_USER=zuko ACCORDO_PAGE=settings ACCORDO_LAYER=keybackup cargo run --features dev-screenshot
```

Self-test with your saved sign-in (counts only, no content, sends nothing):
`ACCORDO_SELFTEST=1 cargo run --features dev-screenshot`.

E2EE compatibility with the web (WebCrypto in Node): `cargo test e2ee` reads
tests/fixtures/webcrypto.json; the other direction:
`ACCORDO_RUST_VECTORS=/tmp/r.json cargo test -- --ignored rust_vectors && node scripts/webcrypto_vectors.mjs check /tmp/r.json`.

Developer tools: **F12** opens a window with every API request (Network), the live connection
(Socket), the app's log (Console) and the module API console (API) — with Developer mode on (App
settings → Developer, remembered), or forced on for a run with `ACCORDO_DEVTOOLS=1`.

The API tab calls the module protocol (the web's mini-app protocol, docs/MINIAPP_API_RU.md) as a
sandbox "Test module": `accordo_handshake`, `ui.*`, `context.get`, `user.*`, `permissions.*`,
`payments.*`. Its registration is a set of switches; what you grant lives in memory. The windows
are the real ones (consent, permissions, links, the token picker without balances, the pay and
cheque windows with "Requested by"), in the app window. The host is `src/module_host.rs`.
In memory only; the Authorization header and secrets in bodies are never recorded. The log also
goes to stderr, filtered by `RUST_LOG` (e.g. `RUST_LOG=accordo_native=debug`).

Tests: `cargo test`; against the real server (public endpoints only, no account):
`cargo test -- --ignored live`.

## Layout (mirrors the web app's `src/`)

```
src/
  main.rs                 main.tsx      window settings, fonts, start
  app.rs                  App.tsx       state, messages, routing sign-in ↔ app
  theme.rs                index.css     palette, fonts, widget styles
  icons.rs                              lucide icons + logo
  api/                    src/api/      every endpoint of the web app's API layer, module for module
                                        (client, auth, chat, cheques, invoices, crowdfund, nft, tasks,
                                        mini_apps, admin, stickers, gifs, articles, webhooks, audit_log,
                                        dashboard, contracts, reconcile) — see api/mod.rs for the map;
                                        plus account (sessions), data (networks, tokens),
                                        session_store (the remembered sign-in)
  e2ee/                    utils/e2ee.ts, vault.ts, e2eeTrust.ts, secretPhrase.ts — chat encryption,
                                        byte-compatible with WebCrypto (ECDH P-256, AES-256-GCM, PBKDF2);
                                        cross-checked both ways by scripts/webcrypto_vectors.mjs
  devtools/               (DevTools)    the F12 panel's buffer: requests, socket events, log; redaction
  player.rs               AudioMessagePlayer.tsx, VideoNotePlayer.tsx — ▶ through the system's
                                        GStreamer (gst-launch-1.0): the sound, a video's frames
  recorder.rs             AudioRecorderBar.tsx, VideoNoteRecorderModal.tsx — a voice message or
                                        video note recorded to MP4 (AAC / H.264), its levels, frame
  store/                  src/store/    client state: app_store (useAppStore: wallets, vault, contacts,
                                        notifications, RPC overrides, per-account data; saved to
                                        <config>/state.json), rooms_store (useRoomsStore)
  components/
    layout.rs             Layout.tsx    top bar + rail + page / bare auth frame
    top_bar.rs            TopBar.tsx    title bar, profile popover
    sidebar.rs            Sidebar.tsx   icon rail
    desktop.rs            desktop/WindowControls.tsx  window buttons, drag, slim title bar
    toast.rs              ToastContext.tsx
    form.rs                             labels, inputs, form card
    page_tabs.rs                        "Write & Claim | Incoming | Outgoing" tabs
    finance/              components/finance/  fee picker, duration picker, token select
    overlay.rs                          modal / drawer on a dimmed backdrop
    wallet_panel.rs       WalletModal.tsx      "Wallet Portfolio" drawer (Vault Not Found)
    key_backup_modal.rs   KeyBackupModal.tsx   "Set your chat password"
    settings/             components/settings/ + SettingsPage cards
      account_profile.rs, chat_encryption.rs, api_access.rs,
      sign_in_security.rs, notifications.rs, connected_apps.rs
    ui.rs                 components/ui icon, avatar, badges, rules
    chat/                 components/chat/
      chat_sidebar.rs     ChatSidebar.tsx
      chat_header.rs      ChatHeader.tsx
      message_list.rs     messages, grouped by author
      composer.rs         message box
      media_cards.rs      AudioMessagePlayer.tsx, VideoNotePlayer.tsx  the card in the feed
      recorder_ui.rs      AudioRecorderBar.tsx, VideoNoteRecorderModal.tsx  while recording
      msg_render/         what a message shows by its type — one file per type
        mod.rs            message_type → the file that draws it
        image.rs, gif.rs, sticker.rs, file.rs, voice.rs, note.rs,
        poll.rs, draw.rs, cheque.rs, invoice.rs, campaign.rs, drop.rs, reply.rs
        picture.rs        the picture box the first three share
      room_avatar.rs
  pages/
    login.rs              LoginPage.tsx
    chat_page.rs          ChatPage.tsx
    settings_page.rs      SettingsPage.tsx  responsive card grid
    cheques_page.rs       ChequesPage.tsx    write a cheque, incoming / outgoing
    invoices_page.rs      InvoicesPage.tsx   issue an invoice, incoming / yours
    drops_page.rs         DropsPage.tsx
    crowdfund_page.rs     CrowdfundPage.tsx  launch, back, protocol
    splash.rs             while a saved session is checked
  dev.rs                  --features dev-screenshot
```

`src/app/` is split by subject: one file per area, each one saying in its header what it is
responsible for. `docs/app-modules.md` holds the map, the rule for what stays in `mod.rs`, and the
recipe (and the gates) for moving the next area out.

## Assets

- `assets/fonts/` — Inter 4.1 (SIL OFL, `LICENSE.txt`), the font the web app loads from Google Fonts.
- `assets/icons/` — lucide icons, written from the web app's own `lucide-react` package by `scripts/lucide2svg.mjs`
  (each icon's `__iconNode` → a standalone SVG with `stroke="#ffffff"`, tinted at runtime).
- `assets/accordo-logo.svg` — copy of `public/accordo-logo.svg`.

## License

Copyright (C) 2026 dazay

This program is free software: you can redistribute it and/or modify it under the terms of
the GNU General Public License as published by the Free Software Foundation, either version 3
of the License, or (at your option) any later version. It is distributed in the hope that it
will be useful, but WITHOUT ANY WARRANTY; see `LICENSE` for the full text.

The "Accordo" name and logo are not covered by this licence — see `TRADEMARKS.md`.
Bundled fonts and icons keep their own licences — see `THIRD_PARTY.md`.
