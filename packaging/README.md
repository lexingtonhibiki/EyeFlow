# Native desktop packaging

Published assets are authoritative only on the GitHub Releases page; CI artifacts are not automatically public releases.
This script assembles Linux/macOS packages; Windows NSIS and publication are
handled separately. See [release scope and validation](../docs/releases/v0.7.4.md).
The native CI matrix assembles test artifacts after successful native checks.
It assembles an already-built binary and never installs software, signs code,
requests credentials, changes system permissions, or publishes an artifact.
Python 3.11+ is required. Run from the repository root.

## Offline checks

```sh
python3 -m unittest discover -s packaging -p 'test_*.py' -v
python3 scripts/package-desktop.py --help
```

These validate metadata, version conversion, staging paths and permissions with
explicitly non-executable fixture bytes. They do **not** validate a real DEB,
Mach-O bundle, GUI, audio, tray or Gatekeeper. Native packaging commands below
fail closed for mismatched architecture, missing tools/libraries and version.
The package version must match Cargo.toml: never label `0.7.4-test.2` as `0.7.4`.
Prerelease `0.7.4-test.2` maps to Debian `0.7.4~test.2`, which sorts before stable.
Apple numeric version keys use `0.7.4`; the exact prerelease is retained in
`EyeFlowBuildVersion`, the DMG filename and the compiled binary.

## Ubuntu 24.04 amd64

Use the native Ubuntu 24.04 build job, not an arbitrary newer glibc host.
Build prerequisites include the existing GTK3/ALSA/AppIndicator/Xdo/fontconfig
headers plus `dpkg-dev`, `desktop-file-utils` and `fonts-noto-cjk`. Packaging
requires installed libraries and their dpkg symbols/shlibs metadata.

```sh
cargo build --release --locked
python3 scripts/package-desktop.py linux --binary target/release/eyeflow --version 0.7.4-test.2
```

The script calls `dpkg-shlibdeps` on the real binary to derive ABI minimums;
missing dependency metadata is fatal (no ignore flag). It additionally declares
GTK3, ALSA, Ayatana AppIndicator3, xdo, fontconfig, Noto CJK, GL/EGL, Wayland,
xkbcommon and xdg-utils, including libraries loaded at runtime rather than
necessarily linked. This is an Ubuntu 24.04 artifact, not a generic Linux promise.
Desktop metadata is checked with `desktop-file-validate`. `dpkg-deb` normalizes
archive ownership to root without sudo. See PLATFORM-NOTES.txt for all installed
paths, capability limitations, and persistent user-config paths.

Native CI should inspect `dpkg-deb --info` and `dpkg-deb --contents`, then install
and remove in a disposable clean Ubuntu 24.04 GUI environment. Validate Settings,
manual rest, CJK glyphs, popup exit, actual sound/device change, X11 lock/fullscreen
and unsupported/unknown state behavior. Do not test installation on the user's
machine as an unrequested side effect. No maintainer scripts or startup service.

## macOS: separate arm64 and x86_64 artifacts

Run on native macOS runners with Xcode command-line tools. Set a deliberate
`MACOSX_DEPLOYMENT_TARGET` for **the build as well as packaging**. The value is a
build constraint, not a tested compatibility claim. Pick a value supported by
all dependencies and actually validate the oldest claimed OS before release.
The current supported packaging target is macOS 15.0+:

```sh
MACOSX_DEPLOYMENT_TARGET=15.0 cargo build --release --locked
python3 scripts/package-desktop.py macos --binary target/release/eyeflow --version 0.7.4-test.2 --arch arm64 --macos-min-version 15.0
```

Use `--arch x86_64` on the Intel runner. `lipo` must report exactly that one
architecture; `otool`'s deployment target must equal the plist minimum.
Non-system dylibs cause an error rather than a silently broken app. The bundle
ID is `io.github.lexingtonhibiki.eyeflow`. The generated `.app` is inside the DMG
with an Applications link and explicit platform notes. `hdiutil verify` checks
container integrity only. SHA256 files are emitted beside both DEBs and DMGs.

The icon is the repository's generated MIT PNG converted with native `sips` and
`iconutil`; no Apple artwork/fonts are redistributed. Upscaling the existing
256px source for large icon representations is intentionally simple, not a new
artwork source. The bundle remains a normal visible app, not LSUIElement-only.

These DMGs are **not Developer ID signed or notarized**. An ad-hoc linker
signature is not a distribution identity. No signing keys/accounts are used and
no Gatekeeper bypass is offered. Native validation must include actual GUI,
CJK rendering, manual rest, popup exit, tray/hotkeys and audio. macOS automatic
reminders pause by default because reliable lock/fullscreen sensing is unsupported.
Explicit compatibility-reminders opt-in accepts those limitations while still
requiring real, fresh idle data. Failed or missing readings remain blocking.
Non-Windows startup is disabled. Shared core does not imply feature parity.

## License/release gate

The repository LICENSE is MIT (copyright 2026 lexingtonhibiki); every artifact
includes it. The icon is generated by `src/icon.rs`/`examples/gen-icon.rs` within
this repository. Linux depends on the distro's Noto package (with its own license
metadata); macOS only discovers installed system CJK fonts on the destination computer.
Neither font is copied. No new bundled dependency is added by this script.
The target-specific normal-dependency license/notice technical audit is complete
for the current graph, including bundled egui fonts. This is not a legal guarantee.
System CJK fonts above are distinct from those embedded fonts. Changes to the
dependency graph require the notice check/audit again. Packages include the
corresponding target notice; PLATFORM-NOTES.txt describes capability differences
without claiming Windows feature parity. BUILD_INFO.json records packaging
checkout information and is not, by itself, proof of binary provenance.

## Primary references checked 2026-10-23

- [Ubuntu noble GTK3 runtime](https://packages.ubuntu.com/noble/libgtk-3-0t64)
- [Ubuntu ALSA runtime](https://packages.ubuntu.com/noble/libasound2t64)
- [Ubuntu Ayatana AppIndicator](https://packages.ubuntu.com/search?keywords=libayatana-appindicator3-1)
- [Ubuntu xdo](https://packages.ubuntu.com/libxdo3)
- [Ubuntu Noto CJK](https://packages.ubuntu.com/source/noble/fonts-noto-cjk)
- [Debian dpkg-shlibdeps](https://manpages.debian.org/bookworm/dpkg-dev/dpkg-shlibdeps.1.en.html)
- [Apple minimum system version](https://developer.apple.com/documentation/BundleResources/Information-Property-List/LSMinimumSystemVersion)
- [Apple bundle metadata](https://developer.apple.com/documentation/bundleresources/bundle-configuration)
