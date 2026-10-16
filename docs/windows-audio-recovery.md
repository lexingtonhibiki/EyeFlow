# Windows audio recovery maintenance patch

## Scope and evidence

Baseline: published v0.7.3, commit `749ab168fdc3342d73400fc01f974a10aaba2194`.
GitHub Releases API checked 2026-10-16: latest release v0.7.3, published
2026-10-13 10:35:22 UTC. This is a maintenance candidate, not a published release.

Reported behavior: reminder sounds and manual preview sometimes both become
silent after headphones are unplugged/replugged; restarting restores sound.
The affected machine's Windows version and actual audio logs are not available.
This report does not claim a reproduced or conclusively diagnosed hardware fault.

Verified code gaps:
- The old AudioPlayer opened one default stream at startup and retained it forever.
- `available()` checked only that an Option was populated, not stream health.
- A startup failure was never retried; preview was disabled by that stale status.
- Custom-file playback returned success without playing anything when no sink existed.
- Rodio's default error handler prints to stderr, not EyeFlow's application log.
- CPAL's WASAPI render loop terminates on several device/buffer errors. Holding a
  live mixer object is not evidence that the rendering loop is still running.

The dependency versions actually locked by this release are rodio 0.22.2 and
CPAL 0.17.3. Relevant upstream sources:
- https://docs.rs/rodio/0.22.2/src/rodio/stream.rs.html
- https://github.com/RustAudio/cpal/blob/v0.17.3/src/host/wasapi/stream.rs
- https://github.com/lexingtonhibiki/EyeFlow/releases/tag/v0.7.3

## Changes

Each reminder or preview releases the previous stream and opens the *current
system-default output device*. Both synthesized and custom sounds take this path.
This also recovers streams that stopped without delivering an error callback.
There is no background polling, automatic replay, new dependency or configuration
migration. Existing sound selection, volume, duration and custom-file duration cap
are retained. This does not alter Windows default-device or volume settings.

A newly requested sound replaces any previous in-progress sound. This deliberately
prevents stale/overlapping previews and releases obsolete endpoints. An unplug
*during* playback may cut off that cue; the next reminder/preview retries, rather
than replaying an obsolete reminder when the device later returns. If the endpoint
is still unavailable, retry again after Windows has finished reconnecting it.

Device-configuration fallback is allowed on the selected default endpoint only;
we do not fall back to a different speaker/device unexpectedly. The prior rodio
convenience function could fall back to other output devices.

Stream errors now go to EyeFlow's log, mark last-known health unavailable, and do
not prevent a later request from retrying. Successful opens are logged too.
Preview remains clickable after failure (custom sounds still need a selected
file), and a successful custom preview clears its previous transient error.
`available()` remains a last-known status, not proof of audible output.

## Verification on the cloud Linux executor

Passed on Rust 1.95.0:
- `cargo fmt --check` and `git diff --check`.
- `cargo check --locked --target x86_64-pc-windows-gnu --no-default-features`.
- `cargo check --locked --tests --target x86_64-pc-windows-gnu --no-default-features`.
- `python3 scripts/test-headless.py`: 80 tests passed using the actual audio,
  configuration, reminder core, UI, translation, statistics, update and wallpaper
  modules, plus locale/tray-ID/document integration tests. There are no domain or
  translation stubs. The 12 audio tests include four output-state tests (retry/drop
  order, startup recovery, immediate error status, old-callback isolation). Three
  new UI tests exercise actual egui pointer events and preview-result state.
  These tests also run through the normal Windows `cargo test` entry point.
- Five repeated parallel headless test runs passed (80 tests each). UI tests share
  the translation tests' test-only lock so labels cannot move between pointer
  frames. Production translation remains lock-free.
- Default-feature `cargo check --locked --tests --target x86_64-pc-windows-gnu`.
- Strict default-feature `cargo clippy --locked --bins --target x86_64-pc-windows-gnu -- -D warnings` (no lint allowances).
- Scoped no-default-feature Clippy also passes with `-D warnings -A unused-variables -A clippy::ptr-arg`; these are verification-only flags, not product attributes.
- `cargo build --release --locked --target x86_64-pc-windows-gnu` passed with default features. The resulting x64 GUI executable includes Windows resources and imports only Windows system DLLs (no separate MinGW runtime DLL needed).
- Independent read-only review checked stream ownership, callback traits, default
  device fallback, custom playback and error recovery.

Remaining limits (the cross-build passed; Windows runtime did not run):
- Strict no-default-features Clippy fails on existing `show_about_tab` unused
  parameters / ptr_arg. The unchanged v0.7.3 baseline reproduces the same failures.
- The initial missing MinGW compiler was resolved by extracting SHA256-verified
  official Debian toolchain packages into the cloud workspace. No system-wide
  installation or user-machine change was made.
- No installer was created and no artifact was signed, installed or published.
- The Windows native full test suite, actual WASAPI output, headphone/Bluetooth
  reconnection and UI latency cannot be validated on this Linux executor.

## Reproducing the supplementary headless suite

On Linux with Python 3.11+, Rust 1.95+, and the ALSA development/link libraries:

```sh
cargo fetch --locked
python3 scripts/test-headless.py
```

The script creates a temporary Cargo package referencing the real source files,
retains dependency selections from the product lockfile, isolates APPDATA in the
temporary directory, and leaves the product Cargo.toml/Cargo.lock unchanged. It
runs offline after the initial fetch. Native Windows/font/GL/output tests are not
part of this supplementary suite. Do not label its 80 passes as the full Windows
application suite. No actual audio stream is opened by the tests.

## Review notes and bounded risks

- CPAL opens shared audio using the default process session and without NOPERSIST.
  Microsoft documents persistent session volume/mute, so reopening should not
  reset the user's mixer setting. This remains a Windows acceptance check, not a
  claim of actual verification: https://learn.microsoft.com/en-us/windows/win32/coreaudio/audio-sessions
- Upstream source risk only, not a reproduced leak or the diagnosed cause here:
  CPAL 0.17.3 `src/host/wasapi/stream.rs:178–188` closes its command-event HANDLE
  only if `push_command(Terminate)` succeeds. If the render thread has already
  exited after an error, its receiver can be gone and that close is skipped.
  Normal healthy replacements send successfully and close the HANDLE; this is
  not a leak on every preview. Observe process handle count during reconnect
  testing. The dependency is not upgraded or patched by this maintenance change.
- Opening/dropping still runs synchronously, as it did at initial startup. Repeated
  playback now pays that cost. Dropping waits for the rendering callback to finish,
  including custom-file decoding; test slow/removable storage and driver latency.
  Moving the whole engine to a worker would be a larger change, deferred here.
- `play_custom` detects a known error during opening or queue submission, but no
  synchronous API can promise the device will remain connected or actually sound
  after it returns. Later failures update last-known health and logs; the next
  explicit cue retries. Each individual device-configuration attempt gets a fresh health flag; old
  callbacks cannot poison either a replacement request or a successful fallback.

## Required Windows acceptance checks before release

On Windows with the normal MSVC/GNU development tools:
1. Run `cargo fmt --check`, `cargo clippy --bins -- -D warnings`,
   `cargo test --locked`, and `cargo build --release --locked` with default features.
2. Test all synthesized presets and a valid custom WAV/MP3 at 50%, 100%, 200%.
   Verify the chosen sound, volume and duration survive all steps below.
3. Preview, unplug/replug headphones, wait for Windows to finish detecting them,
   and preview again *without restarting EyeFlow*. Repeat the cycle at least ten
   times, including switching the default endpoint while the previous one remains
   connected. Observe the actual output device, not just the log.
4. Start with no available output; reconnect one and click Preview. Also fail a
   custom preview, reconnect and succeed: the previous red error must disappear.
5. Test automatic heads-up/start/completion and full-screen sound-only reminders
   after reconnection, not only the manual button.
6. Unplug during a cue/custom file, then issue a new cue after reconnecting. The
   previous sound must not replay; only the new one should play. Rapid previews
   should replace rather than overlap. An invalid custom file should not stop an
   already-running cue merely because decoding failed.
7. Set EyeFlow low/muted in the Windows mixer, trigger repeated previews and
   reconnect the same endpoint. Verify its mixer setting is respected, including
   during recovery. Observe process HANDLE count across reconnect cycles.
8. Check the shortest cues and UI responsiveness with the affected headphone and
   Bluetooth device. Opening/dropping a stream is synchronous on the main thread;
   actual driver latency is an explicit remaining validation risk.
9. If silence persists, collect `%APPDATA%\eyeflow\eyeflow.log` before relaunch
   (the existing logger overwrites it at startup), Windows version, output type,
   default endpoint and exact unplug/replug sequence. Never collect unrelated data.

At the time of local verification, no remote push, PR, release, user-machine
installation or mobile-source change had been made for this patch.
