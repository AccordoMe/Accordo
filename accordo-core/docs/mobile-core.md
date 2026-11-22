# Mobile core (`accordo-core`)

The Android client reuses the desktop client's Rust core instead of re-implementing encryption and
networking in Kotlin. This crate **is** that core: the desktop client depends on it and re-exports
its modules, so one implementation drives both clients.

Why not iced on Android: iced has no mobile target. wgpu and winit can run there, but the platform
layer (soft keyboard and IME, back button, system bars, notifications, permissions, camera and
microphone, file picker, Keystore, deep links, background sync) has to be written anyway. Kotlin
gives that for free, and Compose gives a UI that feels native on a phone. The valuable, hard part —
E2EE, envelopes, API client, wallet signing — lives in Rust and is shared.

## Layout

| Path | Role |
| --- | --- |
| `src/crypto.rs` | E2EE primitives, byte-compatible with the web's WebCrypto (AES-256-GCM, P-256) |
| `src/api/` | The whole API client: auth, chat, cheques, invoices, drops, crowdfund, orgs, socket |
| `src/secrets.rs` | The `SecretStore` trait: where sessions and chat keys live (OS store / Keystore) |
| `src/keystore.rs` | The device's chat key: load, save, delete, moving from older entry names |
| `src/vault.rs` | Password-sealed wallet keys (PBKDF2 600k + AES-GCM) and the sealed-wallet types |
| `src/wallet_session.rs` | The unlock session: derived keys in memory, wiped on lock |
| `src/trust.rs` | TOFU pins for other users' public keys |
| `src/rooms.rs` | Room keys: opening our envelope, wrapping a key for members, archived keys |
| `src/mobile.rs` | The Kotlin-facing facade over the API client: sign-in, rooms, messages, sending, socket |
| `src/mobile/wallet.rs` | Wallet unlock for Kotlin: the master password, the session, `reveal` / `seal` |
| `src/mobile/keys.rs` | Room keys for Kotlin: `RoomKeyring.open` / `wrapFor` / `trustOf` |
| `src/mobile/signup.rs` | Sign-up, waitlist and password recovery for Kotlin (`register`, `resetPassword`, `recoverPassword`) |
| `src/mobile/session.rs` | Saved sign-ins for Kotlin (`saveSession` / `loadSession` / `forgetSession`) and `setConfigDir` |
| `src/log.rs` | `Net` / `Level` record types, secret masking, and the `Sink` trait the host implements |
| `src/lib.rs` | uniffi facade for Kotlin: room keys, seal/open, device keys, envelopes, fingerprints |
| `src/bin/uniffi-bindgen.rs` | Generates the Kotlin bindings from the built library |
| `tests/roundtrip.rs` | Message round trip, envelope between two devices, restored backup, nonce freshness |

The desktop client (`../AccordoDesktop`) has `pub use accordo_core::api;` in `main.rs` and
`pub use accordo_core::crypto::*;` in `src/e2ee/crypto.rs`, so every `crate::api::…` and
`crate::e2ee::crypto::…` path kept working. Its `devtools` module implements
`accordo_core::log::Sink` and installs it in `main()` — the desktop panel and a phone's logcat get
the same records, and the masks live in one place.

## The Kotlin surface

```kotlin
val key = generateRoomKey()
val sealed = encryptText("hello", key)          // String? — base64url, AES-256-GCM
when (val opened = decryptText(sealed!!, listOf(key))) {
    is Decrypted.Text -> println(opened.v1)
    Decrypted.Locked  -> println("no room key on this device")
    is Decrypted.Plain -> println(opened.v1)
}
val me = DeviceKey.generate()
val envelope = me.createEnvelope(key, peerPublicJwk)
```

`src/api/` is exported through `src/mobile.rs`, not as records: payloads cross as **JSON strings**
with the server's own shapes (the ones the desktop deserializes), so the phone's UI can mirror the
desktop screen for screen and a new field needs no binding change.

```kotlin
val client = Client("https://accordo.me", null)            // or Client.remembering(base, token)

val signed = client.login("alina", "password", null)        // suspend → {"token","user"}
val me = JSON.decodeFromString<User>(client.me())           // User is the desktop's struct
val rooms = JSON.decodeFromString<List<ChatRoomItem>>(client.rooms())
val page = JSON.decodeFromString<List<ChatMessageItem>>(
    client.messages(roomKey, beforeId = null, limit = 50L)
)

// Sealing happens in Rust, before the text leaves the phone:
client.sendText(roomKey, "привет", roomKeys.current, null)

// The live connection: events arrive on a background thread.
val sub = client.subscribe(userId, username, object : SocketListener {
    override fun onEvent(event: String) { /* {"kind":"event","event":{…}} */ }
})
sub.stop()
```

Errors arrive as `MobileException.Api(status, message)` / `.SealFailed` / `.Json(message)`.
Signing in keeps the token inside the client, so `rooms()` and the rest just work after `login()`.

## Secrets and keys on the phone

Nothing secret is written by the core itself: it asks the host through
`accordo_core::secrets::SecretStore`. The desktop backs it with the OS credential store
(`AccordoDesktop/src/store/secrets.rs`, the `keyring` crate); the phone implements
`SecretStoreCallback` over the Android Keystore and installs it at startup.

Everything above that trait runs on both clients now: `api::session_store` (saved sign-ins),
`keystore` (the device's chat key), `vault` (password-sealed wallet keys, PBKDF2 600k + AES-GCM),
`wallet_session` (derived keys held in memory, wiped on lock) and `trust` (TOFU pins).

```kotlin
class KeystoreSecrets(context: Context) : SecretStoreCallback { /* EncryptedSharedPreferences */ }

installSecretStore(KeystoreSecrets(context))
check(secretsAvailable())

// The same chat key across launches; a new one on the first launch.
val key = loadChatKey(account, previousAccount, userId, serverPublicKey)
    ?: DeviceKey.generate().also { saveChatKey(account, it) }
```

Android notes: TOFU pins are files under `config_dir` (the phone passes its files directory);
`api::socket` needs a tokio runtime, which `subscribe()` starts on its own thread.

## Wallet unlock

The master password behaves as it does on the web: PBKDF2-HMAC-SHA256 (600 000 iterations, per vault
salt, in parallel), AES-256-GCM over each key, keys in memory only, wiped on `lock()`. Codes are the
same, so a vault made in the browser opens on the phone and back.

```kotlin
val check = newCheckVault(password)                 // first launch only, then store the JSON
val opened = openWalletSession(                     // suspend: ~10⁶ hashes, off the UI thread
    password, check, walletsJson, ttlMinutes = 30u  // walletsJson = the account's SecureWallet[]
)
opened.session.unlocked()                           // true
opened.checkVault                                   // store: new or re-sealed at 600k
opened.upgraded                                     // wallets to sync back (empty JSON array: none)
opened.lockAt                                       // schedule a re-lock, or isExpired() on resume

val privateKey = opened.session.reveal(walletJson)  // one wallet's key (hex)
val vault = opened.session.seal("0x…", opened.checkVault)  // a new wallet, sealed with the session
opened.session.lock()                               // wipe; WalletUnlock.locked() is the locked state
```

A wrong password fails `openWalletSession` with `MobileException.Wallet("Invalid master password")`;
a wallet sealed under another password fails `reveal` with the message the desktop shows. Wallets on
the old iteration count come back re-sealed in `upgraded`, exactly as the web's `vaultUpgrade.ts`.

## Room keys (reading a sealed feed)

```kotlin
val ring = RoomKeyring(user.id, accountKey(user.id, user.username, base))
ring.useDeviceKey(loadChatKey(account, previousAccount, user.id, user.e2eePublicKey)
    ?: DeviceKey.generate().also { saveChatKey(account, it) })

val keys = ring.open(roomJson)          // current first, archived after; [] = room isn't sealed
when (val opened = decryptText(message.content, keys)) {
    is Decrypted.Text -> opened.v1
    is Decrypted.Plain -> opened.v1
    Decrypted.Locked -> "sealed"
}
api.sendText(room.roomKey, text, keys.firstOrNull(), null)   // sealed in Rust when a key is given
```

`open` needs the room JSON as the server sent it: the envelopes (`key_envelopes`) and
`e2ee_meta.archived_keys` are dropped by a trimmed model, so the app keeps the raw string.

The rules are the desktop's (`rooms.rs`, moved into the core, so both clients share them):

- our envelope is opened with this device's key **and the wrapper's** key — the one recorded at wrap
  time, else theirs, else our own;
- the wrapper must be trusted: first sight pins their key, a changed key makes `open` fail with
  `MobileException.Keys("The key for this chat was shared by someone whose chat key changed…")`;
- archived keys come with the current one, sealed under it;
- no envelope we can open, no key — there is no fallback anyone could compute.

`wrap_for(roomKey, members)` produces the `KeyEnvelopes` JSON for a new room key; a member whose key
changed since it was pinned is skipped. `trust_of(userId, jwk)` answers `new`/`trusted`/`verified`/
`changed` for the room-security screen.

## Regenerating the bindings

```bash
cd accordo-core
cargo build
cargo run --bin uniffi-bindgen -- generate \
    --library target/debug/libaccordo_core.so \
    --language kotlin --out-dir ../kotlin-out
```

The generated `.kt` belongs in the Android project as `app/src/main/java/uniffi/accordo_core/`, and
`libaccordo_core.so` goes to `app/src/main/jniLibs/<abi>/`. Bindings and library must come from the
same build.

## Android build

1. `rustup target add aarch64-linux-android x86_64-linux-android`
2. `cargo ndk -t arm64-v8a -t x86_64 -o app/src/main/jniLibs build --release`
3. Add `net.java.dev.jna:jna:5.14.0@aar` (uniffi's runtime).

## Gates

```bash
cargo build && cargo test && cargo clippy --all-targets
# in the desktop crate after any change here:
cd ../AccordoDesktop && cargo check --all-targets && cargo test && cargo clippy --all-targets
```

Plus a fresh bindings run after every change to an exported signature.

## Notes

- Debug `libaccordo_core.so` is ~72 MB; a release build with strip is a few MB per ABI.
- uniffi 0.29.5 generates the bindings in `../kotlin-out/uniffi/accordo_core/`.
