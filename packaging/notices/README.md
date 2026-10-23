# Reviewed target notice inputs

These four target-specific notice texts preserve the license bodies from the
2026-10-23 dependency evidence audit and its independent technical review. The
all-target `candidate` evidence document is intentionally excluded. The header
labels its revision as the license dependency audit baseline, not the packaged
binary source revision. Linux/macOS BUILD_INFO.json records the actual packaging
checkout SHA and tracked-files dirty flag; CI must build the supplied binary from that checkout.
Windows build provenance must also be recorded by its final release assembly.

`manifest.json` records the audited Cargo.lock SHA256, dependency/features
configuration digest, source revision, default-feature selection, exact target
mapping and notice SHA256 values. Run from the repository root:

```sh
python3 scripts/package-desktop.py --check-notices
python3 -m unittest discover -s packaging -p 'test_*.py' -v
```

The current native CI already runs these package tests, including the all-target
notice guard. Windows packaging/release jobs must also run `--check-notices`
before invoking NSIS or assembling a portable archive. Changes to Cargo.lock,
dependency declarations, target dependency configuration or features fail closed;
regenerate/review coverage before updating this manifest. Even a harmless root
version-only lockfile change requires deliberate manifest reconciliation. Do not
update hashes automatically just to make CI pass.

For the 0.7.4 candidate, the lockfile hash was deliberately reconciled after
changing only the root `eyeflow` package version from `0.7.4-test.2` to `0.7.4`.
A structured lockfile comparison found no other package changes; all four target
default-feature runtime dependency graphs matched after excluding the root line.
The license dependency audit baseline remains `719bd5e`; the dependency/features
digest and all four notice texts are unchanged. This is not a new dependency audit.

This guard checks source inputs and files, not binary provenance. Build default
features from this exact checkout with `cargo build --locked --release` and
package that output. Additional features, copied native libraries or altered
embedded assets require fresh coverage review. A technical audit is not a legal
warranty; final toolchain/native-library and installer extraction checks remain.

Package locations:
- Windows NSIS: `$INSTDIR/LICENSE.txt`, `$INSTDIR/THIRD_PARTY_NOTICES.txt`.
- macOS: `EyeFlow.app/Contents/Resources/LICENSE.txt` and `THIRD_PARTY_NOTICES.txt`.
- Linux: `/usr/share/doc/eyeflow/copyright` and `THIRD_PARTY_NOTICES.txt`.
- Windows portable ZIP: the release workflow adds LICENSE.txt and the Windows
  target notice beside eyeflow.exe, and verifies the archived payload before
  publication. NSIS payloads are independently extracted and checked as well.

System fonts are not redistributed. The texts do cover the separately embedded
egui font notices and exact MPL dependency source download URLs. Do not remove
those sections when reducing package size.
