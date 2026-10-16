# EyeFlow portable schedulers

This library contains **two deliberately separate host contracts**:

- `core` and the legacy `eyeflow_*` C ABI compile the existing desktop `src/core.rs` directly. Windows scheduling and tests remain unchanged.
- `mobile` and the new `eyeflow_mobile_*` C ABI implement selected-scope mobile exposure using explicit elapsed intervals. Mobile hosts must use this API, not emulate desktop keyboard-idle signals or manufacture an active clock.

The crate has no egui, Win32, tray, audio, notification, or host filesystem dependency. The host owns sensing, permissions, UI, notifications, and atomic persistence. `include/eyeflow.h` documents both opaque handle types and every ABI signature. Never interchange their handles or snapshots.

```sh
cargo test --locked --manifest-path crates/eyeflow-core/Cargo.toml
cargo clippy --locked --manifest-path crates/eyeflow-core/Cargo.toml --all-targets -- -D warnings
cargo build --locked --release --manifest-path crates/eyeflow-core/Cargo.toml
```

Artifacts are under this crate's `target`, including `release/libeyeflow_core.a` and the target's dynamic library. Rust hosts use public `mobile::{MobileEngine, MobileConfig, MobileSnapshot, MobileState, MobilePhase}`. All calls for one C handle must be serialized; a live handle is destroyed exactly once. No callback or background thread is created.

## Mobile interval contract

Call `eyeflow_mobile_advance(handle, delta_ms, state, delivery_allowed)` for each **disjoint, chronological elapsed interval**. Durations are not cumulative timestamps. The entire interval must be covered by the indicated observation. Split intervals at every known foreground, permission, lock, session, or stop transition. A zero-duration call establishes the current boundary/delivery eligibility without adding time.

| State | Meaning | Exposure | Rest |
| --- | --- | --- | --- |
| `TRACKING` (0) | Selected app(s), positively known foreground and unlocked | Accumulates selected exposure | Never |
| `OUT_OF_SCOPE` (1) | Positively known unselected foreground and unlocked | Paused; debt preserved | Never |
| `LOCKED` (2) | Proven uninterrupted device lock over the entire interval | Paused | Short reset at 30 seconds; long reset at 900 seconds |
| `UNKNOWN` (3) | Missing permission/events, ambiguous state, reboot/process or clock gap | Paused; debt preserved | Never; breaks lock/session continuity |
| `STOPPED` (4) | User stopped observation | Paused; debt preserved | Never |
| `VOLUNTARY_REST` (5) | Explicitly started own-app guided rest UI remains foreground/user-active | Paused | Guided participation only; not proof of eyes looking away |

Switching selected app A → selected app B continues the same selected-scope debt. Switching to unselected apps pauses that debt, but does not forgive it. This is **selected-scope accumulated exposure**, not a claim of observing all device use or physiological strain.

A host must never extrapolate the last known `LOCKED`/`TRACKING` sample across incomplete history. Replay only intervals independently proved by event coverage. For a missing history range, submit `UNKNOWN`. The engine does not infer rest from an interval's size, process absence, a failed permission query, delivery of a notification, or an unselected app.

Defaults retain the original triangular 15–25 minute short interval (peak at 20), 30-second short rest, 15-second short heads-up, 2 hours of accumulated selected exposure before long rest, 900-second long rest, 30-second long heads-up, one 5-minute postponement per round, and 10-minute selected-use retry after skipping a long reminder. Short and long debt are distinct: short resets never clear long exposure. Skips/postpones change the next reminder attempt, not exposure already accrued.

A proven continuous lock reaches the short threshold only once per chain, continues accruing toward the long threshold, and upgrades at 900 seconds without adding another completed session. Shorter locks pause only. Any unlocked/unknown/stopped interval, even a zero-duration boundary, interrupts the lock chain. Long pending reminders survive a 30-second lock; only a sufficient long rest clears them.

## Delivery is not completion

Phases are idle (0), short heads-up (1), short due (2), long heads-up (3), long due (4), short guided/rest intent (5), and long guided/rest intent (6). Heads-up countdowns advance only with selected exposure. Reaching due creates a persistent obligation, **not a running rest timer**. Continued target use can never auto-complete a rest.

`delivery_allowed=false` only disables output. It does not reset, consume, postpone, or silently complete a pending reminder. Historical replay should use `false`; after replay, submit zero elapsed with the current observation and actual notification/channel eligibility. Deliver only when `eyeflow_mobile_can_deliver` is true. Deduplicate by `(eyeflow_mobile_reminder_id, phase)` and record successful delivery only. Retain this marker across foreground changes so repeated target switching does not cause a notification storm. A postponed or skipped/new round receives a new attempt ID.

`START_REST` explicitly arms a voluntary session, but supplies no rest evidence. Only subsequent `VOLUNTARY_REST` intervals can advance guided completion. Target use, out-of-scope navigation, observation loss, or stopping cancels that session without clearing the underlying debt. `LOCKED` intervals can independently satisfy natural rest; partial guided and locked evidence are never added together to manufacture a complete rest. A long session must reach the full long duration, not merely the short threshold. A single long locked interval is not counted as both guided and natural.

Daily counters distinguish guided short/long participation and natural lock chains. `natural_long` is an upgrade subset, not added again to total completed. Counters saturate rather than wrapping. The C wrapper rolls counters using the device's local calendar date; Rust hosts can call `roll_day` explicitly. Rest/debt scheduling is independent of calendar-day rollovers. The current ABI reports aggregate counts, not a full historical timeline.

## Commands and suspension

Commands return 0 when accepted and -1 if inapplicable: 1 start rest, 2 postpone once, 3 skip, 4 pause reminders for one hour, 5 resume, 6 stop tracking, 7 cancel voluntary session. Settle observed intervals before issuing a command. No command manufactures elapsed time.

Pause suppresses delivery only: known selected exposure still accrues, and independently proven lock rest can still reset debt. Pause expiration uses all supplied monotonic elapsed intervals, including `UNKNOWN`, but those intervals receive no rest credit. Resume keeps accumulated debt/pending obligations. Stop disables tracking and rest credit until resume, without deleting the checkpoint or erasing debt. The current phase can remain due while paused/stopped; query `enabled`, `state`, `paused_seconds`, and `can_deliver` instead of interpreting phase alone as delivery permission.

## Checkpoints and conservative rebase

Export/import use bounded UTF-8 JSON with no trailing NUL. Query required length with a null buffer, allocate, then export while holding the same host serialization lock. An undersized buffer is unchanged. Persist atomically and periodically, as well as after commands and completed rests. Failed import is atomic.

Checkpoints preserve configuration, short/long exposure, remaining selected-time deadlines, postponed-once flags, long obligation, attempt IDs, pause/stop state, and daily counters. Import restores to `UNKNOWN` (or `STOPPED`), with delivery disabled. It cancels any previously armed guided session and resets lock continuity: elapsed time while terminated is never granted as rest or active use. Hosts should restart their event cursor conservatively or replay independently proved events. Supply a reliably measured downtime interval as `UNKNOWN` if it should expire a pause; do not use untrusted wall-clock subtraction. Legacy desktop checkpoints are rejected by the mobile API and vice versa.

`eyeflow_mobile_configure` optionally sets fixed durations for deterministic tests/demo mode. Normal defaults stay randomized. Invalid values are rejected, not clamped silently. Exposure is retained and existing due obligations cannot be pushed into the future just by changing settings. Rust tests can configure shorter durations in `MobileConfig` directly.

## Legacy desktop-compatible ABI

The pre-existing `eyeflow_*` API and tests are preserved. This adapter accepts a caller-owned monotonically increasing active clock plus desktop-style idle/focus signals; its scheduler may infer natural rest from desktop inactivity or a large tick gap. It is **not a valid mobile observation contract**. Its snapshot import rebases that legacy active clock to zero. Refer to the first section of the header for signatures.

## Boundary and target checks

After the release build, run the unchanged desktop ABI smoke test:

```sh
cc -Wall -Wextra -Werror -I crates/eyeflow-core/include \
  crates/eyeflow-core/tests/abi_smoke.c \
  -L crates/eyeflow-core/target/release -leyeflow_core \
  -Wl,-rpath,"$PWD/crates/eyeflow-core/target/release" -o /tmp/eyeflow-abi-smoke
/tmp/eyeflow-abi-smoke
```

`tests/mobile_policy.rs` independently covers selected A→B, unselected/unknown/stopped debt preservation, denied delivery, no rest from ongoing target use, partial/interrupted lock chains, 30/900-second thresholds, bulk/chunked interval equivalence, guided participation, postpone/skip, pause/stop, conservative restoration, and malformed inputs. Rust unit tests also check the C API and deterministic triangular sampler.

Mobile compile checks are useful but **not substitutes for NDK/Xcode linkage, signed apps, real permissions, or device lifecycle testing**:

```sh
cargo check --locked --manifest-path crates/eyeflow-core/Cargo.toml --target aarch64-linux-android
cargo check --locked --manifest-path crates/eyeflow-core/Cargo.toml --target aarch64-apple-ios
```
