#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
command -v cargo >/dev/null || { echo 'Install Rust from https://rustup.rs'; exit 1; }
cargo ndk --version >/dev/null 2>&1 || { echo 'Install cargo-ndk: cargo install cargo-ndk --locked'; exit 1; }
: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to your installed Android NDK directory}"
rustup target add aarch64-linux-android x86_64-linux-android
cd "$ROOT/crates/eyeflow-core"
# Rust cdylibs do not automatically carry a SONAME on Android. Give the
# linker/loader a stable runtime dependency name instead of a build-tree path.
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C link-arg=-Wl,-soname,libeyeflow_core.so -C link-arg=-Wl,-z,max-page-size=16384"
cargo ndk -t arm64-v8a -t x86_64 --platform 26 -o "$HERE/app/src/main/jniLibs" build --locked --manifest-path "$ROOT/crates/eyeflow-core/Cargo.toml" --release
