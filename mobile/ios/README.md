# EyeFlow for iPhone / iPad

Native SwiftUI prototype, iOS 17+, backed by the shared Rust scheduler. **Not compiled or run in Xcode, simulator or on a signed device in this Linux authoring environment.**

## 当前功能边界

用户要求的是目标 App 的**连续使用**提醒，并在**真正锁屏休息足够久**后重置：至少 30 秒重置短休息，至少 15 分钟重置长休息。每日累计使用阈值不能代替这个需求。

本版尚无经过验证、受支持的跨应用连续使用与实际锁屏时间线实现。因此跨应用提醒功能明确标记为**暂不可用**，不以每日累计、离开 EyeFlow 或后台墙钟时间进行近似。没有应用选择、授权入口、每日阈值、自动遮罩或通知。

保留的是 EyeFlow 内的主动休息辅助：开始休息、推迟、跳过、暂停、手动专注及本机统计。计时只在 EyeFlow 前台推进；离开应用、锁屏或被系统打断时暂停，回来后继续。离开应用不被算作完成休息或真实锁屏休息，不声称实现了上述跨应用重置规则。

## Retired prototype migration

The previous daily cumulative DeviceActivity integration is disabled in source, not merely hidden:

- No `startMonitoring`, event/schedule creation, picker, Screen Time authorization request, shield opt-in control, shield assignment, or notification scheduling remains.
- App launch and every foreground activation call `LegacyUsageCleanup.disable()`: turn off persisted monitor/shield flags, stop EyeFlow's old named registration, clear only EyeFlow's named ManagedSettings store, and discard old pending cues, selected tokens and threshold settings.
- The monitor extension is retained solely as an **inert cleanup extension** for previously registered callbacks. All six callback overrides perform the same cleanup and never create a cue or shield.
- Family Controls/App Group entitlements and the embedded extension remain for legacy cleanup compatibility, not to expose a new monitoring capability. This code does not revoke system account permissions or change other apps' restrictions.
- An already installed old binary cannot be changed remotely. Cleanup takes effect when this updated app/extension runs. Use the same bundle IDs, App Group and named store to migrate an existing installation. Signed-device verification is still required.

## Own-app implementation

SwiftUI presents the shared core's short/long-break schedule and voluntary controls. A monotonic clock advances only during an active scene. Rust JSON snapshots preserve remaining durations, phase and statistics; restoration rebases the host elapsed origin to zero. Local daily totals describe only app activity, not total phone use or verified physical rest. Snapshots save every five foreground seconds and on lifecycle/commands; forced termination can lose recent unsaved time. No network/account/cloud sync is required.

## Build on macOS

Install Xcode with the iOS SDK, Rust and XcodeGen from official sources. From this directory:

```sh
./scripts/build-core.sh
xcodegen generate
xcodebuild -project EyeFlow.xcodeproj -scheme EyeFlow \
  -sdk iphonesimulator -configuration Debug CODE_SIGNING_ALLOWED=NO build
open EyeFlow.xcodeproj
```

The script builds arm64 device, Apple Silicon simulator and Intel simulator Rust static libraries and packages `Frameworks/EyeFlowCore.xcframework`. The Swift bridge uses the canonical shared-core header. No binary is committed. Choose your developer team and signing configuration for a physical device. For migration, keep the original app/extension bundle IDs and registered App Group (`group.com.eyeflow.mobile` here) consistent in both entitlements and `Shared/ActivityStorage.swift`.

Retaining the legacy Screen Time extension may require Family Controls capability/provisioning. Distribution capability approval, signing, publication and account changes have not been performed. These build requirements do not mean cross-app continuous-use detection is supported.

## Checks

```sh
python3 tests/check-source.py
bash -n scripts/build-core.sh
```

These are static source safety, plist and shell checks, **not compilation or runtime tests**. Assertions reject cumulative registration, enrollment controls, automatic shielding and notifications; require startup/foreground cleanup and inert extension callbacks; and check the voluntary foreground clock wiring.

Device acceptance checks still required:

1. Upgrade an old installation with monitoring/shield flags and a pending cue. Launch: old registration is stopped, EyeFlow's shield is cleared, old cue/selection discarded. No monitor/picker/shield-enable entry remains.
2. Deliver each legacy interval/event/warning callback: cleanup only, no cue, shield or notification. Repeated cleanup is safe and does not clear another app's settings store.
3. Fresh install and launch without Screen Time approval: no authorization prompt; own-app rest controls remain usable.
4. During own-app idle, heads-up and break, switch apps or lock for 30 seconds and 15 minutes. Return: foreground countdown resumes without claiming actual lock detection or cross-app rest reset. Nothing appears on the lock screen.
5. Test voluntary start, postpone, skip, focus and pause; skipped breaks do not count as complete. Kill/relaunch to verify saved durations restore without offline-time credit.
6. Verify Dynamic Type, VoiceOver, narrow layouts, iPad split view, midnight and timezone changes.

Apple references for the retained cleanup mechanisms: [DeviceActivityCenter](https://developer.apple.com/documentation/deviceactivity/deviceactivitycenter), [DeviceActivityMonitor](https://developer.apple.com/documentation/deviceactivity/deviceactivitymonitor), [ManagedSettingsStore](https://developer.apple.com/documentation/managedsettings/managedsettingsstore). No claim is made that these APIs supply a reliable cross-app lock/unlock timeline.
