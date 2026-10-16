#!/usr/bin/env bash
# Linux/JDK host validation only; this does not build or test an Android APK.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
CORE="$ROOT/crates/eyeflow-core"
ANDROID="$ROOT/mobile/android"
python3 "$ANDROID/tests/check-reminder-wiring.py"
: "${JAVA_HOME:?Set JAVA_HOME to a JDK 17 or newer}"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
cargo build --manifest-path "$CORE/Cargo.toml" --release
CXX="${CXX:-g++}"
"$CXX" -std=c++17 -Wall -Wextra -Werror "$ANDROID/tests/host_contract.cpp" \
  -I"$CORE/include" -L"$CORE/target/release" -leyeflow_core \
  -Wl,-rpath,"$CORE/target/release" -o "$OUT/host-contract"
"$OUT/host-contract"
"$CXX" -std=c++17 -Wall -Wextra -Werror -fPIC -shared \
  -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/linux" -I"$CORE/include" \
  "$ANDROID/app/src/main/cpp/eyeflow_jni.cpp" -L"$CORE/target/release" -leyeflow_core \
  -Wl,-rpath,"$CORE/target/release" -o "$OUT/libeyeflow_jni.so"
SOURCES=("$ANDROID/app/src/main/java/com/eyeflow/mobile/UsageClock.kt" \
 "$ANDROID/app/src/main/java/com/eyeflow/mobile/NativeCore.kt" \
 "$ANDROID/app/src/main/java/com/eyeflow/mobile/ReminderDelivery.kt" \
 "$ANDROID/app/src/main/java/com/eyeflow/mobile/ReminderPresentationPolicy.kt" \
 "$ANDROID/tests/ReminderDeliveryTest.kt" \
 "$ANDROID/tests/ReminderPresentationPolicyTest.kt" \
 "$ANDROID/tests/UsageClockTest.kt" "$ANDROID/tests/NativeSmoke.kt")
if command -v kotlinc >/dev/null; then
  kotlinc -jvm-target 17 -include-runtime -d "$OUT/tests.jar" "${SOURCES[@]}"
  CLASSPATH="$OUT/tests.jar"
else
  : "${GRADLE_HOME:?Set GRADLE_HOME to a Gradle distribution, or install kotlinc}"
  STDLIB="$(find "$GRADLE_HOME/lib" -maxdepth 1 -name 'kotlin-stdlib-[0-9]*.jar' -print -quit)"
  "$JAVA_HOME/bin/java" -cp "$GRADLE_HOME/lib/*" org.jetbrains.kotlin.cli.jvm.K2JVMCompiler \
    -no-stdlib -no-reflect -classpath "$STDLIB" -jvm-target 17 -d "$OUT/tests.jar" "${SOURCES[@]}"
  CLASSPATH="$OUT/tests.jar:$STDLIB"
fi
"$JAVA_HOME/bin/java" -cp "$CLASSPATH" com.eyeflow.mobile.UsageClockTestKt
"$JAVA_HOME/bin/java" -Djava.library.path="$OUT" -cp "$CLASSPATH" com.eyeflow.mobile.NativeSmokeKt
