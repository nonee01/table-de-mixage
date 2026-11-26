#!/usr/bin/env bash
# Construit l'appli Android « Micro table » : le cœur Rust (AAudio) puis l'APK.
set -euo pipefail
cd "$(dirname "$0")"
NDK="${ANDROID_HOME:-/opt/android-sdk}/ndk/$(ls "${ANDROID_HOME:-/opt/android-sdk}/ndk" | sort -V | tail -1)"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android28-clang"
(cd rust && cargo build --release --target aarch64-linux-android)
mkdir -p app/src/main/jniLibs/arm64-v8a
cp rust/target/aarch64-linux-android/release/libmicro_telephone.so app/src/main/jniLibs/arm64-v8a/
./gradlew -q assembleDebug
ls -la app/build/outputs/apk/debug/app-debug.apk
