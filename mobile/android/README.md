# EyeFlow Android 0.3.0

Native Kotlin UI + JNI + the dedicated Rust **mobile evidence engine**. The previous
0.1.0 accumulated-use prototype is superseded. An APK build alone does not establish
real-device background/UsageEvents reliability; run the acceptance checks below.

## Product rules

- Opt in to Android Usage Access, select launchable apps, allow notifications, and
  explicitly start/resume. The app cannot grant its own permissions. Package/activity
  transitions are processed locally; no screen text, video content, or network access.
- TRACKING: unlocked selected-app use increases short and long exposure. Switching
  selected A → selected B continues the same exposure, not a new daily bucket.
- OUT_OF_SCOPE: an unselected app pauses exposure, **never counts as rest**, and
  preserves outstanding exposure and pending reminders.
- LOCKED: verified screen-off/keyguard-locked intervals count as rest. One continuous
  30-second lock resets short exposure; one continuous 900-second lock clears long
  exposure. Short interrupted locks cannot be summed to manufacture a completed rest.
- UNKNOWN: permission loss, clock discontinuity, missing/incomplete history, ambiguous
  foreground and unproved process/sampling gaps preserve debt and suppress reminders.
- STOPPED: explicit Stop preserves the session/debt and does not award rest. Resume
  continues it; Start/Resume explicitly resumes paused reminders as well. Process
  absence is not silently treated as a completed rest; a pause is not promised to
  remain active across an explicit Start/Resume action.
- Voluntary rest requires tapping **现在休息** and staying in the focused EyeFlow UI
  (no split screen or PiP). Target use interrupts it; merely displaying a notification
  or letting a countdown run while still watching a target app never completes rest.
  Locks and voluntary UI rest are separate evidence chains, not combined partial rests.

Defaults remain a triangular short interval of 15–25 minutes, peak 20 minutes; short
rest 30 seconds; long rest 15 minutes after two hours selected exposure. Postpone and
Skip do not prove rest. Pause reminders for one hour suppresses delivery only: target
use still accumulates and supplied monotonic elapsed time expires the pause.

## Event reconstruction and sleep

UsageEvents timestamps are aligned to elapsedRealtime using a checked wall-clock
anchor. Every supplied interval is disjoint and chronological. The service replays
historical intervals with delivery disabled, then evaluates delivery at the current
state. It does not substitute the last observed app for the whole polling interval.

A normal <=30-second sample reconstructs package and lock transitions. A longer gap
is UNKNOWN **except** for historical lock spans backed by explicit screen/keyguard
boundaries in a successful timestamp-aligned query. A previously recorded lock plus
an explicit historical unlock/screen-on can prove the whole sleeping interval. A new
explicit lock plus a currently-locked sensor can bound an ongoing historical span.
Screen-on terminates off-screen proof even without a keyguard-hidden event. Screen-on
alone does not prove unlock; keyguard/current sensors establish a new state from now.
No events, a failed query, lost permission or a clock jump cannot fabricate a long
rest. No wake lock or battery exemption is requested. If an OEM omits evidence, the
app deliberately undercounts instead of clearing exposure incorrectly.

## Delivery and persistence

The prewarning channel is silent. The due channel defaults to a single short vibration
with no sound (system settings and DND take precedence); silent and sound modes are
explicit choices. No full-screen notification or automatic Activity launch exists. Notifications are cancelled outside eligible target use and while
locked. Notification permission/channel denial preserves pending work. Successful
posts are deduplicated by stable reminder ID plus phase, persisted with the session.
Cancelling outside the target clears visibility, not alert history. Returning to the
same pending round silently restores its actionable notification using group-alert
suppression; repeated active ticks do not repost. A restart can silently restore one
current pending reminder, never a backlog. Heads-up
and due phases have different content. An opt-in, separately permissioned center card
is bounded to 320dp / 88% screen width, times out in 10 seconds, never takes focus and
does not intercept touches outside its own small window. See
`docs/mobile-progressive-reminders.md` for UX boundaries and acceptance.

Android requires a foreground-service session control; it remains a silent status
notification, with SECRET lockscreen visibility. Rest alerts are also SECRET and
rechecked against PowerManager/KeyguardManager before posting. OS delivery races,
Do Not Disturb and OEM restrictions cannot be eliminated or promised away.

Version-2 mobile snapshots and matching counters are stored together in private
SharedPreferences every 15 seconds, on commands/screen events, and graceful shutdown.
Import preserves exposure/pending work but drops lock and voluntary-rest continuity.
A recent unsaved interval may be lost on process death. Stop does not delete the
snapshot. Old desktop-scheduler snapshots/statistics are not valid mobile proof; a
one-time visible migration notice explains their exclusion and removes old keys.

## Build

Use JDK17, Gradle8.11.1, AGP8.9.2, Kotlin2.1.20, Android SDK35, NDK27.0.12077973 and
CMake3.22.1. Install Rust stable and cargo-ndk. Set ANDROID_HOME and ANDROID_NDK_HOME.
Accept SDK licenses yourself or through an explicitly authorized setup flow. Preserve
the same debug signing key when updating an existing test installation; never put it
in source archives. No third-party runtime UI framework is used.

```sh
cargo install cargo-ndk --locked
./build-rust.sh
gradle :app:assembleDebug :app:lintDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

The script outputs arm64-v8a and x86_64 Rust libraries, imported/packaged by CMake.
Version code4 / version name0.3.0 retain application ID com.eyeflow.mobile.

## Candidate package CI

The proposed `Android debug package (not device acceptance)` workflow uses one
Ubuntu 24.04 job (30-minute timeout; superseded runs cancelled). It runs host
policy/JNI tests, builds both Android ABIs, compiles and lints the app, and checks
the actual APK plus deliberately broken packaging fixtures before uploading a
7-day debug artifact. A successful job is not emulator/device acceptance or a
release. The per-run debug key may not update an existing installation; try a
disposable emulator first, never uninstall an existing app just to bypass signing.

CI explicitly selects the hosted image's preinstalled NDK 27.3.13750724 and CMake
3.31.5 with `-PeyeflowNdkVersion` / `-PeyeflowCmakeVersion`; local defaults remain
27.0.12077973 / 3.22.1. The preflight requires JDK17, SDK35, build-tools35.0.0 and
these native tools already installed. It does not accept licenses or install SDK
packages, and disables Gradle's SDK auto-download. A changed runner image fails
closed for review instead of silently selecting a different toolchain.

## Host verification

```sh
JAVA_HOME=/path/to/jdk GRADLE_HOME=/path/to/gradle ./tests/run-host-tests.sh
```

The script compiles production UsageClock policy tests, the actual mobile C ABI
contract, and a Kotlin→JNI→Rust snapshot/state round trip. It is not Android device
instrumentation. Tests cover target→target→other transitions inside one poll,
permission loss during lock, unknown long gaps, proven 900-second sleep/unlock,
screen-on without keyguard-hidden, interrupted lock chains, no completion while
continuing target use, STOP debt preservation and conservative restart.

## Device acceptance (required before release)

- Grant/revoke Usage Access and notification permission separately. Disabling the
  rest channel must keep pending reminders without claiming a completed rest.
- Select reader+video. Switch A→B mid-poll, then unselected app; verify exact state
  intervals and preserved debt, not natural credit for the unselected app.
- Lock29s → unlock → lock1s: no reset. Lock30s: short reset only. Lock900s: long reset.
- Let the device really sleep between screen-off and unlock: verify historical lock
  boundaries reconstruct the span without a wake lock. Missing logs stay UNKNOWN.
- Exercise keyguard/no-keyguard, screen-on still locked, split-screen, PiP, OEM apps,
  clock changes, force-stop/relaunch, background restriction and battery saver.
- Reach due while continuing target use: no automatic completed-rest count. Start
  voluntary rest in EyeFlow; return to target before completion: retain pending debt.
- Stop/resume and process death: preserve debt/pending, discard continuity proof, no
  duplicate totals. Upgrade0.1→0.2: see migration notice, exclude old rest claims.
- Bounce between target/non-target: no repeat alert storm; a new reminder round or
  heads-up→due transition may post once. Lock must suppress the current alert.

The specialUse foreground service needs a Google Play declaration/policy review
before public distribution; approval is not guaranteed. This is an eye-rest reminder,
not medical treatment and not proof of actual eye movement or physiology.

### 0.2.1 native packaging validation

The Rust shared runtime is explicitly included from `app/src/main/jniLibs`.
The JNI bridge's CMake import declares `libeyeflow_core.so` as its SONAME.
`assembleDebug` now runs `tests/verify-apk-native.py` against the **actual APK**.
It requires both arm64-v8a and x86_64, checks ELF architecture and SONAMEs, and
walks every packaged library's DT_NEEDED dependencies recursively. Only an
explicit set of public Android API-26 system libraries may remain external.
Python 3 is required (`-PnativeVerifierPython=/path/to/python` can override it).

Independent check:

    python3 tests/verify-apk-native.py app/build/outputs/apk/debug/app-debug.apk --readelf /path/to/ndk/llvm-readelf

A packaging check is necessary but does not replace installing the APK and
starting an actual EyeFlow session on Android. Capture logcat when initialization
fails; the `EyeFlowSession` tag includes the underlying exception, without logging
saved session data. Verify upgrade signing certificates before installation.
A newly generated debug signing key cannot update an installation signed with
another key. Do not uninstall a user's existing app without their permission;
uninstallation erases app data. Use a disposable emulator for that test.

Regression-test the gate (removes each ABI core and injects a missing transitive dependency):

    python3 tests/test-apk-native-verifier.py app/build/outputs/apk/debug/app-debug.apk
