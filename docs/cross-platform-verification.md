# Cross-platform verification record

Date: 2026-10-15. Development branch, not a production release.

## Superseded prototype

The first Android APK (versionCode 1, SHA-256 `7c57850fa8c1eef74d9e4abf7bf1bd7d9b381b0466aadb75bdceb2b9e4c46b6f`) and original iOS cumulative monitor do **not** satisfy the clarified continuous-use requirement. The earlier tests proved compilation and internal consistency, not correct product semantics: unselected apps and revoked usage access were incorrectly allowed to count as rest. Do not treat the earlier package as the recommended implementation.

The correction separates target use, out-of-scope, actual lock, unknown and stopped states; only supported lock evidence or an explicit guided session can discharge rest obligations. Daily cumulative iOS monitoring and its automatic shield are removed, including legacy cleanup. See `mobile-continuous-use.md` for the decision and acceptance rules.

## Corrected verification

- Rust shared crate: 53 tests pass (23 retained desktop/legacy tests, 30 new mobile policy/ABI tests).
- Strict clippy for all core targets passes; Android ARM64 and iOS ARM64 Rust target checks pass.
- New Android production interval reconstruction, mobile C ABI contract and Kotlin → JNI → actual Rust library host tests pass. These test the corrected API, not only the old desktop adapter.
- iOS static checks verify no cumulative enrollment/registration/automatic shield and launch/foreground/legacy callback cleanup; shell syntax checks pass.
- Independent design review rechecked debt preservation, lock boundaries, guided interruption, long postponement, iOS retirement, reminder visibility/alert separation and inclusive UsageEvents query boundaries; no remaining P0/P1 blockers in the reviewed source. Reviewer independently ran 26 mobile policy tests and iOS static checks.
- Additional Kotlin → JNI → Rust regression proves two 15-second locks separated by a zero-duration unlock/relock do not combine into a 30-second rest.
- Delivery policy tests cover due → out-of-scope cancellation → silent restoration, no repeated active-tick post, no locked delivery, persisted marker and one initial post per new phase/round.
- Corrected Android APK build passes; final lint and package digest are recorded below after the last regression/rebuild.
- `scripts/verify-portable.sh` passes, including Linux portable desktop check, legacy C ABI smoke and corrected Android mobile C ABI host contract.
- Physical-device behavior and iOS Xcode compilation remain unverified.

## Historical prototype execution (not corrected acceptance)

- Rust shared scheduler: 23 unit tests passed, including 17 inherited scheduler behavior tests and new snapshot/FFI validation.
- `cargo clippy --all-targets -- -D warnings` for shared core: passed.
- Shared core release build: passed.
- C client linked to the actual Linux shared library: passed.
- Rust type/target checks for `aarch64-linux-android` and `aarch64-apple-ios`: passed. These are not native SDK linkage or mobile app builds.
- Portable desktop host: `cargo check`, `cargo build`, and strict clippy passed on Linux. No GUI session or desktop OS runtime inspection was available.
- Android production `UsageClock.kt`: compiled and ran on JVM; target app, inactive/locked/revoked inputs, natural-gap, resting, backward-clock, and restored clock checks passed.
- Android JNI source: strict C++ compilation passed against actual JDK JNI headers.
- Android `NativeCore.kt` loaded compiled C++ JNI + actual Rust release library: create/tick/command/statistics/export/import/destroy smoke passed on Linux JVM.
- Android C ABI host timing contract: passed for frozen exposure, natural breaks and silent rest completion.
- iOS project/entitlement plist and YAML parsing, shell syntax, static consent/lifecycle/shield/default-OFF checks: passed.

Reproduce portable checks with `scripts/verify-portable.sh`; Android host test instructions are under `mobile/android/tests/run-host-tests.sh`.

## Historical prototype Android package build

Android SDK/NDK debug APK packaging passed with API 35, NDK 27, AGP 8.9.2 and Kotlin 2.1.20. Android lintDebug reports no issues. ARM64 and x86_64 Rust/JNI libraries are included. This is a debug-signed test APK, not a release-signed/store build.

## Still required before release readiness

- Install and exercise Android permission flows, UsageEvents ordering and missing events, actual lock/unlock and screen-on races, selected-app transitions, background sleep, service shutdown/restart, notification channels, OEM restrictions, clock jumps and upgrade migration on physical devices.
- Build/sign the iOS foreground-only app and inert legacy extension in Xcode; check that upgrade cleanup stops old monitoring and removes this app's shields without touching other apps.
- A supported evidence chain for cross-app continuous use and true lock rest must be established before iOS automatic monitoring can be considered. Signing entitlements alone do not solve it.
- Validate macOS/Windows portable desktop compile/runtime if distributing that preview. The original Windows GUI/installer was not built here.

## Product limits

Android reads package/screen transitions, not screen contents or eye movement. Missing or unreliable evidence is UNKNOWN, not rest. Its notification-only reminder is not an overlay; Android timing/delivery and OEM limits still require device verification. iOS cross-app continuous monitoring is unavailable; no daily cumulative substitute or automatic shield is offered. The desktop portable preview only counts its own foreground session, not the original Windows global sensing experience.

## Superseded Android artifact

- File: `app-debug.apk`, 2,672,420 bytes.
- SHA-256: `7c57850fa8c1eef74d9e4abf7bf1bd7d9b381b0466aadb75bdceb2b9e4c46b6f`.
- APK v2 signature verification passed; 16 KiB ZIP and ELF LOAD alignment passed.
- Both JNI libraries refer to `libeyeflow_core.so` by library name, without build-machine absolute paths.
- Android SDK license accepted only after the owner's explicit approval: https://developer.android.com/studio/terms . No unrelated SDK/preview/emulator agreements were accepted.

## Corrected Android artifact: use this instead of version 0.1.0

- Version 0.2.0, versionCode 2; application ID `com.eyeflow.mobile`.
- File: `EyeFlow-Android-continuous-debug.apk`, 1,564,585 bytes.
- SHA-256: `0a4282590973f8a8bdcef9c95f0879e18dad405dd94f58beff6c592016d8b5ce`.
- Final corrected host suites, `assembleDebug` and `lintDebug` passed; lint reports **No issues found**.
- APK v2 signature verified; certificate exactly matches the earlier debug APK, enabling an ordinary update subject to device policy.
- ARM64-v8a and x86_64 libraries included; all four ELF libraries and ZIP entries pass 16 KiB alignment; JNI dependencies use `libeyeflow_core.so`, not build-machine paths.
- No INTERNET permission. No emulator or physical-device execution has been performed.
- SDK agreement remains the previously owner-approved Android SDK license; no new agreements were accepted during this correction.
