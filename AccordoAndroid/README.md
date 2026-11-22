# Accordo for Android

The phone client is a Jetpack Compose app on top of the shared Rust core (`../accordo-core`). The
look must match the desktop client, so nothing visual is invented here: the palette is generated from
the desktop's own `theme.rs`, and the data comes through the core in the server's JSON — the same
shapes the desktop screens read.

## What is here

| Path | Role |
| --- | --- |
| `theme/AccordoTheme.kt` | The desktop palette as Compose colors, generated — do not edit by hand |
| `scripts/theme_to_kotlin.py` | Reads `AccordoDesktop/src/theme.rs`, rewrites the palette |
| `scripts/make_icons.py` | Draws the launcher icon (adaptive + legacy mipmaps) from the logo's geometry |
| `icons/logo.svg`, `icons/logo-mark.svg` | The web's logo, and the same file without its background |

```bash
python3 AccordoAndroid/scripts/theme_to_kotlin.py   # after any change to the desktop palette
python3 AccordoAndroid/scripts/make_icons.py        # after any change to the logo
```

The icon is drawn, not traced: `make_icons.py` reproduces `public/accordo-logo.svg` (two round-capped
strokes and a dot on a 64-grid, indigo→violet→sky gradient) with Pillow — legacy mipmaps at 48…192 px
plus adaptive layers for API 26+ (full-bleed gradient behind, the mark inside the safe zone). The
gradient is stretched across the visible circle, so a launcher's mask does not cut it to one colour.

## Building

```
AccordoAndroid/
  settings.gradle.kts        modules and repositories
  build.gradle.kts           versions: AGP, Kotlin, Compose
  app/build.gradle.kts       SDK levels, Compose, JNA (uniffi's runtime), serialization
  app/src/main/java/me/accordo/mobile/   the app: core/, data/, model/, theme/, ui/
  app/src/main/java/uniffi/accordo_core/accordo_core.kt   generated bindings (copied)
  app/src/main/jniLibs/<abi>/libaccordo_core.so           the Rust library (cargo ndk)
```

```bash
# 1. bindings and the library for the phone
cd accordo-core
cargo build
cargo run --bin uniffi-bindgen -- generate --library target/debug/libaccordo_core.so \
    --language kotlin --out-dir ../kotlin-out --no-format
cp ../kotlin-out/uniffi/accordo_core/accordo_core.kt ../AccordoAndroid/app/src/main/java/uniffi/accordo_core/
rustup target add aarch64-linux-android x86_64-linux-android
cargo ndk -t arm64-v8a -t x86_64 -o ../AccordoAndroid/app/src/main/jniLibs build --release   # needs the NDK

# 2. the app
cd ../AccordoAndroid
./gradlew :app:assembleDebug       # or `gradle :app:assembleDebug`
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

`sdk.dir` in `AccordoAndroid/local.properties` points at the SDK (not committed). The Java toolchain comes
from Gradle (foojay), so a machine with only a JRE still builds — the first run downloads JDK 21.

## Wiring

1. `cargo ndk -t arm64-v8a -t x86_64 -o app/src/main/jniLibs build --release` in `accordo-core`.
2. Copy `kotlin-out/uniffi/accordo_core/accordo_core.kt` into `app/src/main/java/uniffi/accordo_core/`.
3. Add `net.java.dev.jna:jna:5.14.0@aar` and `kotlinx-serialization-json` to Gradle.
4. Copy `theme/AccordoTheme.kt` into `app/src/main/java/me/accordo/mobile/theme/`.

## Screens, mirroring the desktop

The desktop window is a rail (accounts, organizations), a chat list, the feed, and the composer under
it. The phone keeps the same order top to bottom: chat list → feed → composer, with the rail as a
drawer, so a screen can be compared with the desktop window side by side.

What the core already gives each screen:

| Screen | Call |
| --- | --- |
| Sign in / up / recover | `login`, `register`, `registrationStatus`, `resetPassword`, `recoverPassword` |
| Chat list | `rooms()` / `conversations()`; local filter over title and last message; avatars |
| Feed | `messages(roomId, beforeId, limit)`; avatars, reactions (`react`), pinned marks (`isPinned`), thread hints |
| Thread | `thread(roomId, rootId)` → replies; `sendRaw` with `thread_root_id` to answer |
| Profile | `me()`, `updateProfile(displayName)`, `fingerprintOf(e2ee_public_key)`, sign out |
| Sending | `sendText(roomId, body, roomKey?, replyToId?)` — sealed in Rust when `roomKey` is given |
| Live updates | `subscribe(userId, username, listener)` → `{"kind":"event","event":…}` on a background thread |

Opening a message: `decryptText(payload, roomKeys)` returns `Text` / `Locked` / `Plain`, exactly the
three states the desktop draws. `roomKeys` come from `RoomKeyring.open(roomJson)` — see below.

## Staying signed in

The token lives in the Keystore-backed secret store (`saveSession`), together with a pointer to the
last account; `loadSession` + `me()` on start puts the app back in the chat list, and an expired
token is forgotten silently. Signing out (`forgetSession`) removes both. `setConfigDir(filesDir)`
gives the core a place for the TOFU pins, so a changed chat key is remembered across restarts too.

## Signing in, signing up, getting back in

The sign-in screen has the desktop's three forms, in one column:

| Form | What it does |
| --- | --- |
| Sign in | `Client.login`; the one-time-code field appears **only** when the server answers 403 `twoFactorRequired()` — 2FA accounts, not everyone |
| Sign up | `registrationStatus(base)` first: paused → waitlist (`joinWaitlist`); invite-only → access code instead of email; otherwise email + username + password + terms (`termsVersion()`); a fresh chat key's public half goes with the sign-up, the private one into the Keystore; email-less accounts get recovery codes shown once |
| Forgot password | `forgotPassword` sends a 6-digit code, `resetPassword` takes it with a new password; `recoverPassword` does the same with a recovery code |

## Room keys

```kotlin
val ring = RoomKeyring(user.id, accountKey(user.id, user.username, base))
ring.useDeviceKey(loadChatKey(account, previousAccount, user.id, user.e2eePublicKey)
    ?: DeviceKey.generate().also { saveChatKey(account, it) })

val keys = ring.open(roomJson)        // current first, archived after; [] = the room isn't sealed
```

The envelopes that carry the key are opened with this device's key and the wrapper's, and only if
this device trusts the wrapper (first sight pins it; a changed key refuses the envelope —
`MobileException.Keys` with the sentence the feed shows). Sending into a sealed room passes
`keys.first()` to `sendText`, so the text is sealed inside Rust.

## Looking at the screens without an account

`scripts/mock_server.py` answers the handful of calls the app makes on the way in — sign-in, `me`,
rooms, a page of messages (one pinned, one with reactions, one with a thread), the thread itself and
the toggles — with the same JSON the real server sends. No account on a real server is needed to walk
the UI:

```bash
python3 AccordoAndroid/scripts/mock_server.py 8099     # then type http://10.0.2.2:8099 as the Server
```

## Secrets and keys

Nothing secret is left to the app's discretion: the core asks the host where to keep it.

```kotlin
class KeystoreSecrets(context: Context) : SecretStoreCallback {
    // EncryptedSharedPreferences backed by the Android Keystore
    override fun get(key: String): String? = …
    override fun set(key: String, value: String): Boolean = …
    override fun remove(key: String) = …
}

installSecretStore(KeystoreSecrets(context))     // before the first session read
val key = loadChatKey(account, previousAccount, userId, serverPublicKey)
    ?: DeviceKey.generate().also { saveChatKey(account, it) }
```

Saved sign-ins (`session:<id>:<nick>@<server>`) and the chat key live in that store, so they survive
a restart without any file in the app's data directory.

## Wallet unlock

```kotlin
val check = newCheckVault(password)                  // first launch; store the JSON afterwards
val opened = openWalletSession(password, check, walletsJson, ttlMinutes = 30u)   // suspend
val key = opened.session.reveal(walletJson)          // hex key of one wallet
val vault = opened.session.seal("0x…", opened.checkVault)
opened.session.lock()                                // or let lockAt / isExpired() decide
```

## Status

Running, checked on the emulator (`Pixel_7`, API 34, x86_64) on this machine:

```
sdkmanager "ndk;27.0.122xxxxx"      # or unzip android-ndk-r27c into $ANDROID_HOME/ndk/
cargo install cargo-ndk
rustup target add x86_64-linux-android aarch64-linux-android
./scripts/build-apk.sh --install     # bindings → .so → APK → adb install → launch
```

`app-debug.apk` is ~31 MB with both `libaccordo_core.so` (x86_64 for the emulator, arm64-v8a for
phones). The sign-in screen talks to the real server: a wrong password shows the server's own
`Invalid credentials`, so Kotlin → uniffi → Rust → TLS → accordo.me works end to end.

TLS: the core builds its own `rustls` config with the bundled Mozilla roots (`webpki-roots`). reqwest's
default `rustls` feature asks the *platform* for certificates, which on Android needs the
rustls-platform-verifier JNI glue — without it every request fails with `Expect
rustls-platform-verifier to be initialized`. Bundled roots behave the same on desktop and phone; the
cost is that enterprise/self-signed CAs are not trusted, which is fine for one server.

Two things the generated bindings can't do, learned while building:

- an exported method may not be called `close` (the class already implements `AutoCloseable.close()`)
  — the socket subscription's is `stop()`;
- an error variant may not have a field called `message` (it collides with `Throwable.message`) — the
  core's error fields are `detail` (read them, not `e.message`, which prints `status=…, detail=…`).

## Not here yet

- `config_dir` for TOFU pins: pins are files under the config dir, so on Android they don't survive a
  restart until the app passes its files directory — within a session a changed key is already
  refused.
- Signing in Rust: today the phone gets the private key and signs in Kotlin; moving signing into the
  core would keep the key inside Rust entirely.
- Creating or rotating a sealed room from the phone: `wrap_for` is exported, the screens are not.

