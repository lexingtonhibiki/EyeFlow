#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test --locked --manifest-path crates/eyeflow-core/Cargo.toml
cargo clippy --locked --manifest-path crates/eyeflow-core/Cargo.toml --all-targets -- -D warnings
cargo build --locked --release --manifest-path crates/eyeflow-core/Cargo.toml
cargo check --locked --manifest-path desktop/portable/Cargo.toml
python3 mobile/ios/tests/check-source.py
if [[ "$(uname -s)" == Linux ]]; then
  TEMP="$(mktemp -d)"
  trap 'rm -rf "$TEMP"' EXIT
  cc -Wall -Wextra -Werror -I crates/eyeflow-core/include crates/eyeflow-core/tests/abi_smoke.c \
    -L crates/eyeflow-core/target/release -leyeflow_core \
    -Wl,-rpath,"$PWD/crates/eyeflow-core/target/release" -o "$TEMP/abi"
  "$TEMP/abi"
  c++ -std=c++17 -Wall -Wextra -Werror -I crates/eyeflow-core/include mobile/android/tests/host_contract.cpp \
    -L crates/eyeflow-core/target/release -leyeflow_core \
    -Wl,-rpath,"$PWD/crates/eyeflow-core/target/release" -o "$TEMP/host"
  "$TEMP/host"
fi
printf 'Portable checks passed. APK/IPA builds and device behavior need separate verification.\n'
