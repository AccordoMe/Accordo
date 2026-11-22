# Accordo

Accordo is an end-to-end encrypted messenger with crypto payments built into the chat: cheques,
invoices (including split bills), crowdfunding campaigns and token drops on EVM networks.

This repository holds the native clients and the Rust core they share. The server and the web app
are not part of it: the clients talk to an Accordo server, `https://accordo.me` by default.

## What's here

| Folder | What it is | Built with |
| --- | --- | --- |
| [`accordo-core/`](accordo-core/) | The shared core: API client, live socket, end-to-end encryption, room keys, wallet vault, Kotlin bindings | Rust, [uniffi](https://mozilla.github.io/uniffi-rs/) |
| [`AccordoDesktop/`](AccordoDesktop/) | The desktop client | Rust, [iced](https://iced.rs) 0.14 (no WebView) |
| [`AccordoAndroid/`](AccordoAndroid/) | The Android client | Kotlin, Jetpack Compose |

```
AccordoDesktop ── Rust crate ───┐
                                ├── accordo-core ── HTTPS + WebSocket ──► Accordo server
AccordoAndroid ── uniffi (JNA) ─┘
```

Both clients go through the same core, so encryption, the API and the server's data shapes are
implemented once. The Android client mirrors the desktop screen for screen: its palette is generated
from the desktop's `theme.rs`, and its icons are the same lucide set.

## Status

- **Desktop** is the main client. The first release is for **Linux (x86_64)**; on Windows and macOS
  Accordo is used through the web app for now.
- **Android** is early: sign-in and sign-up, chats (encrypted included), threads, reactions,
  profile, settings, the network picker and wallet unlock work. Payments (cheques, invoices,
  crowdfunding, drops) are not on the phone yet.

## Privacy and keys

- Private chats and groups are encrypted on the device (ECDH P-256, AES-256-GCM). The server
  relays and stores only ciphertext. The format is byte-compatible with the web app's WebCrypto,
  checked by test vectors in both directions.
- The chat key lives in the system's secret store: Secret Service / GNOME Keyring / KWallet on
  Linux, the Android Keystore on the phone.
- Wallet keys are sealed with the user's master password (PBKDF2 + AES-GCM) and stay on the device.

## Building

You need **Rust 1.95 or newer** from [rustup](https://rustup.rs). The `rustc` from distro
packages is usually too old.

### Desktop (Linux)

```sh
cd AccordoDesktop
cargo build --release          # → target/release/accordo-native
cargo run --release
```

- `scripts/install-linux.sh` adds Accordo to the app menu with its icon (`~/.local`, no sudo).
- `scripts/package-linux.sh` makes `dist/accordo-<version>-linux-x86_64.tar.gz` for people
  without Rust.
- Voice messages and video notes use the system's GStreamer: `gstreamer1.0-tools` and
  `gstreamer1.0-plugins-{base,good,libav,ugly}`. Without it the app still runs.
- Another server: type it on the sign-in screen, or start with
  `ACCORDO_API_URL=http://127.0.0.1:8000`.

More (testing with two windows, developer tools, layout): [`AccordoDesktop/README.md`](AccordoDesktop/README.md).

### Android

Needs the Android SDK with NDK r27, `cargo-ndk` and the Android Rust targets. Gradle downloads the
JDK itself.

```sh
cargo install cargo-ndk
rustup target add aarch64-linux-android x86_64-linux-android
AccordoAndroid/scripts/build-apk.sh            # bindings → libaccordo_core.so → APK
AccordoAndroid/scripts/build-apk.sh --install  # …then install and start it on a device or emulator
```

The script finds the SDK in `$ANDROID_HOME` (or `~/Android/Sdk`) and the NDK in
`$ANDROID_NDK_HOME` (or `<SDK>/ndk/android-ndk-r27c`). `AccordoAndroid/local.properties` (`sdk.dir=…`) is per machine and not
committed. More: [`AccordoAndroid/README.md`](AccordoAndroid/README.md).

### Core

```sh
cd accordo-core
cargo test
cargo clippy --all-targets
```

After a change to the core, check the desktop too
(`cd ../AccordoDesktop && cargo check --all-targets && cargo test`), and regenerate the Kotlin
bindings when an exported signature changed (`build-apk.sh` does it). How the core is shaped for
Kotlin: [`accordo-core/docs/mobile-core.md`](accordo-core/docs/mobile-core.md).

## Documentation

- [`AccordoDesktop/README.md`](AccordoDesktop/README.md): building, running, developer tools,
  source layout
- [`AccordoDesktop/docs/app-modules.md`](AccordoDesktop/docs/app-modules.md): how the desktop
  app's state is split by area
- [`AccordoAndroid/README.md`](AccordoAndroid/README.md): the phone client, its screens and what
  the core gives each one
- [`accordo-core/docs/mobile-core.md`](accordo-core/docs/mobile-core.md): the shared core and its
  Kotlin surface

## License

Copyright (C) 2026 dazay

The code is free software under the GNU General Public License, version 3 or later; the full text
is in [`AccordoDesktop/LICENSE`](AccordoDesktop/LICENSE). The licence covers the code only, not the
"Accordo" name and logo: see [`AccordoDesktop/TRADEMARKS.md`](AccordoDesktop/TRADEMARKS.md).
Bundled fonts and icons keep their own licences: see
[`AccordoDesktop/THIRD_PARTY.md`](AccordoDesktop/THIRD_PARTY.md).
