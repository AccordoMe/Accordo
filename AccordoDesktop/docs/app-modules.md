# The app module, split by subject

`src/app/` holds the client's single `App` type: all state, the `Message` enum, `update`, `view`
and `subscription`. As features accumulated it reached 3200 lines with one `impl App` block, so the
per-area handlers were moved out into one file per subject. This document is the rule for keeping
it that way and the recipe for the next move.

## Map

| file                | responsible for                                                        | lines |
|---------------------|------------------------------------------------------------------------|-------|
| `mod.rs`            | `App` (fields), `Message`, `update` dispatcher, `view`, `subscription`, and the socket events it routes | ~1300 |
| `keys.rs`           | keyboard input: hotkeys (the user's own, `hotkeys.rs`), recording new ones, Ctrl+K switcher, Tab/Esc/F12, zoom, hiding | 250 |
| `avatar.rs`         | the account's avatar: URL, upload, save                                | 80 |
| `lists.rs`          | cheques / invoices lists, the time window, the chat's attach window     | 200 |
| `settings.rs`       | app settings: the window's messages, load and save                     | 110 |
| `wallet.rs`         | wallets, the vault (master password), key backup, locking              | 330 |
| `picker.rs`         | token picker, chain settings (RPC endpoints, explorers, faucets)        | 395 |
| `tx.rs`             | transaction review, the `Effect`s pages ask for, the signing wallet     | 270 |
| `flows.rs`          | cheque / invoice / drop / crowdfund flows (the `start_*` handlers)      | 330 |
| `session.rs`        | signing in (`enter`), leaving, switching accounts, the privacy screen  | 170 |
| `windows.rs`        | what the window does: title-bar actions, close / blur, dropped files   | 80 |
| `loads.rs`          | what the loaded data does: chains, tokens, wallets, the store          | 130 |
| `msg/`              | the message types of the areas: `msg/session.rs` (SessionMsg), `msg/window.rs` (WindowMsg), `msg/data.rs` (DataMsg) | 110 |
| `module_console.rs` | the module API console in Developer tools                              | 505 |
| `orgs.rs`           | organizations on the rail                                              | 219 |

The split is done: 3200 lines in one file became ~1000 of core plus twelve area files. What is still
in `mod.rs` is there by design (next section).

## What stays in `mod.rs`, and why

- `struct App` — the fields are the single source of truth; they cannot be split without moving
  state, which is a different (and much larger) refactor.
- `enum Message` — the dispatcher's input, one list.
- `update` — it matches on `Message` and delegates: `Message::Wallet(m) => self.wallet_update(m)`.
  Keep it a dispatcher; logic belongs in the area files.
- `view`, `subscription`, `title` — the frame around every page.

Everything else is fair game for an area file.

## How it works (Rust mechanics)

Three facts make this cheap:

1. **Inherent `impl` blocks may live in any module of the same crate.** `impl App { … }` in
   `app/keys.rs` is the same `App` as in `app/mod.rs`.
2. **Privacy runs downward.** A child module (`app::keys`) can read and write the private fields of
   a type defined in its parent (`App`), so `self.session`, `self.toasts` … work unchanged.
3. **`use super::*;`** brings in what the parent module sees — the imports at the top of `mod.rs` —
   so a moved method needs no import list of its own.

The one thing that must change on a move: a method the **parent** calls has to be
`pub(super) fn`, because a method defined in a child module is private to that module and the
parent is not a descendant of it. Private helpers used only inside the child file stay plain `fn`.

## Recipe for moving one area out

1. Pick the subject and the methods (see the table above).
2. Create the file with a header (`// SPDX…`, `//!` what it is responsible for and what it is not),
   `use super::*;`, and an empty `impl App { }`.
3. Move the methods with the script — it copies the block byte for byte, rewrites the signature to
   `pub(super) fn`, and appends it inside the `impl` block:

   ```sh
   python3 scripts/extract-app-method.py src/app/mod.rs \
       wallet_update=src/app/wallet.rs \
       vault_update=src/app/wallet.rs
   ```

4. Add `mod wallet;` next to the other `mod` lines in `mod.rs` (after `mod keys;` etc.).
5. Move the doc comments **and the `#[cfg]` attributes**: anything above `fn x` in `mod.rs` does
   **not** travel with the method (the script cuts from the `fn` line). A `///` left behind silently
   documents the *next* function (clippy: `empty_line_after_doc_comments`); a `#[cfg(feature = …)]`
   left behind makes the moved method unconditional, which still compiles with default features and
   only breaks in a build without them. Move both, then check the feature matrix (below).
6. Run the gates.

## Gates (run after every move)

```sh
cargo check --all-targets                            # 0 errors; the warning count must not grow
# The optional money sections: cfg attributes must travel with their methods.
for f in "" "--no-default-features" \
         "--no-default-features --features drops" \
         "--no-default-features --features crowdfund"; do
    cargo check --all-targets $f || echo "FAILED: $f"
done
cargo clippy --all-targets 2>&1 | grep -c 'src/app/'   # 0 findings in the touched directory
cargo test                                           # same count as before the move (118)
cargo build                                          # linkage, not just types
wc -l src/app/*.rs
```

The moves are mechanical, so the gates are the whole safety net: unit tests cover the logic modules
(e2ee, recording, payloads) but **no test covers the UI**. A green build proves the wiring compiles,
not that a view still looks right — say so when reporting, and let the user click through the areas
that were touched.

## Rules of thumb

- One file per *subject*, not per method. `setting.rs` for settings, not `load_settings.rs`.
- If a handler needs helpers from another area, leave them where they are and call them; do not
  duplicate. Cross-area access is normal — the state is shared.
- New feature, new handler: put it in the area file straight away. `mod.rs` only grows for
  cross-cutting things.
- Keep the header honest: say what the file is responsible for **and** what lives elsewhere. The
  next reader (or agent) trusts the header instead of grepping 2700 lines.

## Deep phase: nested message enums (started)

`Message` had 69 flat variants and `update` matched 78 arms, 60 of them with logic inline. The
pattern now in use: an area's messages live in its own file under `app/msg/`, the area's file holds
the handler, and `Message` keeps a single variant that dispatches to it.

```rust
// app/msg/session.rs — the types, nothing else
#[derive(Debug, Clone)]
pub enum SessionMsg { Logout, SwitchTo(String), Loaded(String, Option<Saved>), /* … */ }

// app/session.rs — what they do
pub(super) fn session_update(&mut self, m: SessionMsg) -> Task<Message> { /* the moved arms */ }

// app/mod.rs
Session(SessionMsg),                                   // in `enum Message`
Message::Session(m) => return self.session_update(m),   // in `update`
```

Types and handlers are apart on purpose: the enum is what the views and subscriptions construct, and
reading it is the fastest way to learn what the area can do — it should not be buried under 200 lines
of handler. `app/msg/mod.rs` re-exports them (`pub use msg::{DataMsg, SessionMsg, WindowMsg};`), so a
view writes `use crate::app::SessionMsg;` and never the full path.

Done so far: **session** (10 variants → 1), **window** (6 → 1), **data arriving** (11 → 1) — the types
in `msg/session.rs`, `msg/window.rs`, `msg/data.rs`, the handlers in `session.rs`, `windows.rs`,
`loads.rs`. `update` went from 78 arms to 48; `mod.rs` from 3200 to ~1000 lines. Large payloads get
boxed (`Restored(Result<Box<Session>, ApiError>)`) or clippy reports `large_enum_variant`.

Next groups:

| group        | variants to fold                                                        |
|--------------|-------------------------------------------------------------------------|
| `NavMsg`     | ToggleRail, Navigate                                                    |
| wallet       | OpenWallet, CloseWallet, WalletClock → into `wallet_panel::WalletMsg`    |
| `Logout`-less leftovers | CopyText, SelectChain, NotYet, DismissToast, StoreSaved, TxReady, Claim* |

Two traps, both hit while doing this:

- **Rename longest-first is wrong.** `Message::FocusNext` → `Message::Window(WindowMsg::FocusNext(…))`
  and then the later `Message::Window` pass wraps it again. Rewrite the *prefix* variant
  (`Window`) first, or tokenize once instead of chaining replaces.
- **Never rewrite the dispatcher arm you just inserted.** `Message::Window(m) => self.window_update(m)`
  must keep taking the area enum; a blanket `Message::X` rewrite turns it into
  `WindowMsg::Action(m)` and breaks the types.

Recipe per group: move the arms into a `*_update` fn in the area file (same verbatim move as
`scripts/extract-app-method.py`, then rename `Message::X` → `XMsg::X` inside the moved text), add
the nested variant + one dispatcher arm, and rewrite the construction sites with a balanced-paren
rewrite — `Message::X(a)` → `Message::Area(XMsg::X(a))`, and a bare `Message::X` used as a task
mapper → `|v| Message::Area(XMsg::X(v))`. Unit variants take no closure; large payloads get boxed.

## The shared core (`../accordo-core`)

`src/api/` and `src/e2ee/crypto.rs` are no longer in this crate: they moved to `accordo-core`, which
the Android client links as well. Paths did not change — `main.rs` has `pub use accordo_core::api;`
and `src/e2ee/crypto.rs` is `pub use accordo_core::crypto::*;`, so `crate::api::…` and
`crate::e2ee::crypto::…` keep resolving.

The core must stay free of iced and of this crate's `store`: it reports through
`accordo_core::log::Sink`, which `devtools` implements and `main()` installs. New code in `api/`
takes plain data in and returns plain data out; anything that needs a widget handle belongs here, in
the desktop layer.
