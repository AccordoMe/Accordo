#!/usr/bin/env bash
# Builds the phone app end to end: Kotlin bindings, the Rust library for each ABI, the APK.
# With --install it also installs and launches on the attached device or emulator.
#
#   ./scripts/build-apk.sh              # bindings + .so + app-debug.apk
#   ./scripts/build-apk.sh --install    # …and adb install -r + launch
#
# Needs: Rust with the Android targets, cargo-ndk, the NDK, the Android SDK, a JDK (Gradle fetches one).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CORE="$ROOT/accordo-core"
MOBILE="$ROOT/AccordoAndroid"
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$SDK/ndk/android-ndk-r27c}"
INSTALL=false
[ "${1:-}" = "--install" ] && INSTALL=true

echo "1/3  Kotlin bindings"
cd "$CORE"
cargo build --quiet
cargo run --quiet --bin uniffi-bindgen -- generate \
    --library target/debug/libaccordo_core.so --language kotlin --out-dir "$ROOT/kotlin-out" --no-format
mkdir -p "$MOBILE/app/src/main/java/uniffi/accordo_core"
cp "$ROOT/kotlin-out/uniffi/accordo_core/accordo_core.kt" "$MOBILE/app/src/main/java/uniffi/accordo_core/"

echo "2/3  libaccordo_core.so for x86_64 and arm64-v8a"
cargo ndk -t x86_64 -t arm64-v8a -o "$MOBILE/app/src/main/jniLibs" build --release --lib
ls -la "$MOBILE/app/src/main/jniLibs"/*/libaccordo_core.so

echo "3/3  the APK"
cd "$MOBILE"
./gradlew --no-daemon :app:assembleDebug
APK="$MOBILE/app/build/outputs/apk/debug/app-debug.apk"
ls -la "$APK"

if $INSTALL; then
    ADB="$SDK/platform-tools/adb"
    "$ADB" install -r "$APK"
    "$ADB" shell am start -n me.accordo.mobile/.MainActivity
    echo "Started; follow the log with: $ADB logcat -s Accordo RustStdio AndroidRuntime"
fi
