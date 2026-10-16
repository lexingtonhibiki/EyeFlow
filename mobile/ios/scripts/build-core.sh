#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
IOS="$ROOT/mobile/ios"
MANIFEST="$ROOT/crates/eyeflow-core/Cargo.toml"
TARGET="$ROOT/crates/eyeflow-core/target"
[[ "$(uname -s)" == Darwin ]] || { echo 'Requires macOS with Xcode and Rust.' >&2; exit 1; }
command -v xcodebuild >/dev/null
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
for triple in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
  cargo build --manifest-path "$MANIFEST" --target-dir "$TARGET" --release --target "$triple"
done
mkdir -p "$IOS/Frameworks" "$IOS/.build/simulator" "$IOS/.build/headers"
cp "$ROOT/crates/eyeflow-core/include/eyeflow.h" "$IOS/.build/headers/"
xcrun lipo -create \
  "$TARGET/aarch64-apple-ios-sim/release/libeyeflow_core.a" \
  "$TARGET/x86_64-apple-ios/release/libeyeflow_core.a" \
  -output "$IOS/.build/simulator/libeyeflow_core.a"
# xcodebuild refuses an existing output. Keep deletion narrowly scoped to our artifact.
rm -rf "$IOS/Frameworks/EyeFlowCore.xcframework"
xcodebuild -create-xcframework \
  -library "$TARGET/aarch64-apple-ios/release/libeyeflow_core.a" -headers "$IOS/.build/headers" \
  -library "$IOS/.build/simulator/libeyeflow_core.a" -headers "$IOS/.build/headers" \
  -output "$IOS/Frameworks/EyeFlowCore.xcframework"
echo 'Built Frameworks/EyeFlowCore.xcframework'
