#!/usr/bin/env python3
"""Assemble native test artifacts. Does not build, install, sign, or publish."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
BUNDLE_ID = "io.github.lexingtonhibiki.eyeflow"
RUNTIME = ("libgtk-3-0t64", "libasound2t64", "libayatana-appindicator3-1",
           "libxdo3", "libfontconfig1", "fonts-noto-cjk", "libgl1", "libegl1",
           "libwayland-client0", "libxkbcommon0", "libxkbcommon-x11-0", "xdg-utils")


def notice_for(target, root=ROOT):
    """Fail closed if audited inputs or selected notice differ; not a legal verdict."""
    directory = root / "packaging/notices"
    manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
    if manifest["feature_selection"] != {"default_features": True, "extra_features": []}:
        raise ValueError("Only the audited default-feature build is supported")
    if hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest() != manifest["cargo_lock_sha256"]:
        raise ValueError("Cargo.lock changed: refresh and review target notices before packaging")
    cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    dependencies = {key: cargo.get(key, {}) for key in ("features", "dependencies", "build-dependencies", "target")}
    digest = hashlib.sha256(json.dumps(dependencies, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if digest != manifest["manifest_dependency_configuration_sha256"]:
        raise ValueError("Cargo dependency/features configuration changed: review notice coverage")
    entry = manifest["targets"][target]
    label = "windows" if target == "x86_64-pc-windows-msvc" else target
    expected = f"THIRD_PARTY_NOTICES.{label}.txt"
    if entry["file"] != expected:
        raise ValueError("Notice filename must match the exact target, never a candidate file")
    notice = directory / expected
    if notice.is_symlink() or not notice.is_file():
        raise ValueError(f"Missing regular target notice: {expected}")
    if hashlib.sha256(notice.read_bytes()).hexdigest() != entry["sha256"]:
        raise ValueError(f"Notice changed since review: {expected}")
    return notice


def build_info(version, target):
    """Record the packaging checkout, never relabel the license audit as a build."""
    manifest = json.loads((ROOT / "packaging/notices/manifest.json").read_text())
    return json.dumps({
        "version": version,
        "target": target,
        "package_checkout_revision": run("git", "-C", ROOT, "rev-parse", "HEAD"),
        "package_checkout_tracked_files_dirty": bool(run("git", "-C", ROOT, "status", "--porcelain", "--untracked-files=no")),
        "license_dependency_audit_baseline_revision": manifest["license_dependency_audit_baseline_revision"],
        "cargo_lock_sha256": manifest["cargo_lock_sha256"],
        "feature_selection": manifest["feature_selection"],
        "provenance_requirement": "Input binary must be built with default features from this checkout; packaging alone cannot verify binary provenance.",
    }, indent=2) + "\n"


def version_parts(value):
    match = re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*))?", value)
    if not match:
        raise ValueError("Expected a numeric three-part version, optionally with a prerelease suffix")
    core = ".".join(match.group(1, 2, 3))
    return core, core + ("~" + match[4] if match[4] else "")


def run(*args, cwd=None):
    return subprocess.check_output([str(x) for x in args], cwd=cwd, text=True).strip()


def copy(source, dest, executable=False):
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, dest)
    dest.chmod(0o755 if executable else 0o644)


def write(dest, data):
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(data, encoding="utf-8")
    dest.chmod(0o644)


def plist(version, minimum):
    core, _ = version_parts(version)
    if not re.fullmatch(r"[0-9]+\.[0-9]+(?:\.[0-9]+)?", minimum):
        raise ValueError("Invalid macOS minimum version")
    return dict(CFBundleIdentifier=BUNDLE_ID, CFBundleName="EyeFlow",
                CFBundleDisplayName="EyeFlow", CFBundleExecutable="eyeflow",
                CFBundlePackageType="APPL", CFBundleIconFile="EyeFlow.icns",
                CFBundleShortVersionString=core, CFBundleVersion=core,
                EyeFlowBuildVersion=version, LSMinimumSystemVersion=minimum,
                NSHighResolutionCapable=True,
                NSHumanReadableCopyright="Copyright © 2026 lexingtonhibiki; MIT License")


def stage_linux(stage, binary, version, dependencies):
    notice = notice_for("x86_64-unknown-linux-gnu")
    _, deb_version = version_parts(version)
    copy(binary, stage / "usr/bin/eyeflow", True)
    copy(ROOT / "assets/icon-preview.png", stage / "usr/share/icons/hicolor/256x256/apps/eyeflow.png")
    copy(ROOT / "packaging/eyeflow.desktop", stage / "usr/share/applications/eyeflow.desktop")
    copy(ROOT / "LICENSE", stage / "usr/share/doc/eyeflow/copyright")
    copy(notice, stage / "usr/share/doc/eyeflow/THIRD_PARTY_NOTICES.txt")
    write(stage / "usr/share/doc/eyeflow/BUILD_INFO.json", build_info(version, "x86_64-unknown-linux-gnu"))
    copy(ROOT / "packaging/PLATFORM-NOTES.txt", stage / "usr/share/doc/eyeflow/PLATFORM-NOTES.txt")
    size = sum(p.stat().st_size for p in (stage / "usr").rglob("*") if p.is_file())
    write(stage / "DEBIAN/control", f"""Package: eyeflow
Version: {deb_version}
Architecture: amd64
Maintainer: lexingtonhibiki <42111195+lexingtonhibiki@users.noreply.github.com>
Section: utils
Priority: optional
Installed-Size: {(size + 1023) // 1024}
Depends: {dependencies}
Homepage: https://github.com/lexingtonhibiki/EyeFlow
Description: Context-aware eye-care break reminder (platform preview)
 Ubuntu 24.04 amd64 preview; native capability validation remains required.
 See /usr/share/doc/eyeflow/PLATFORM-NOTES.txt for limitations.
""")


def linux(stage, binary, version, output):
    if sys.platform != "linux":
        raise ValueError("Build the DEB on native Ubuntu 24.04 amd64")
    release = dict(line.split("=", 1) for line in Path("/etc/os-release").read_text().splitlines() if "=" in line)
    if release.get("ID", "").strip('"') != "ubuntu" or release.get("VERSION_ID", "").strip('"') != "24.04":
        raise ValueError("This package dependency baseline is Ubuntu 24.04 only")
    header = binary.read_bytes()[:20]
    if len(header) < 20 or header[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", header, 18)[0] != 62:
        raise ValueError("Expected a Linux ELF64 little-endian x86_64 executable")
    # Let installed shlibs/symbols metadata determine actual ABI minimums.
    work = stage.parent / "shlib-work"
    write(work / "debian/control", "Source: eyeflow\nSection: utils\nPriority: optional\nMaintainer: lexingtonhibiki <42111195+lexingtonhibiki@users.noreply.github.com>\nStandards-Version: 4.7.0\n\nPackage: eyeflow\nArchitecture: any\nDescription: EyeFlow\n")
    shlibs = run("dpkg-shlibdeps", "-O", "-e" + str(binary), cwd=work)
    derived = next((line.removeprefix("shlibs:Depends=") for line in shlibs.splitlines() if line.startswith("shlibs:Depends=")), "")
    if not derived:
        raise ValueError("dpkg-shlibdeps did not produce dependencies")
    stage_linux(stage, binary, version, ", ".join([derived, *RUNTIME]))
    run("desktop-file-validate", stage / "usr/share/applications/eyeflow.desktop")
    artifact = output / f"eyeflow_{version_parts(version)[1]}_amd64.deb"
    if os.path.lexists(artifact):
        raise ValueError(f"Refusing to overwrite {artifact}")
    run("dpkg-deb", "--build", "--root-owner-group", stage, artifact)
    return artifact


def mac(stage, binary, version, output, arch, minimum):
    if sys.platform != "darwin":
        raise ValueError("Build app/DMG on native macOS with Xcode command-line tools")
    if run("lipo", "-archs", binary).split() != [arch]:
        raise ValueError("Mach-O architecture does not match requested single architecture")
    commands = run("otool", "-l", binary)
    found = re.findall(r"\bminos\s+([0-9.]+)", commands) or re.findall(r"cmd LC_VERSION_MIN_MACOSX\s+cmdsize \d+\s+version ([0-9.]+)", commands)
    normalize = lambda v: tuple(int(x) for x in v.split(".")) + (0,) * (3 - len(v.split(".")))
    if len(found) != 1 or normalize(found[0]) != normalize(minimum):
        raise ValueError(f"LSMinimumSystemVersion {minimum} must match binary deployment target {found}")
    for line in run("otool", "-L", binary).splitlines()[1:]:
        dependency = line.strip().split(" (", 1)[0]
        if not dependency.startswith(("/usr/lib/", "/System/Library/")):
            raise ValueError(f"Unbundled non-system dylib: {dependency}")
    target = "aarch64-apple-darwin" if arch == "arm64" else "x86_64-apple-darwin"
    notice = notice_for(target)
    app = stage / "EyeFlow.app"
    copy(binary, app / "Contents/MacOS/eyeflow", True)
    resources = app / "Contents/Resources"
    copy(ROOT / "LICENSE", resources / "LICENSE.txt")
    copy(notice, resources / "THIRD_PARTY_NOTICES.txt")
    write(resources / "BUILD_INFO.json", build_info(version, target))
    copy(ROOT / "packaging/PLATFORM-NOTES.txt", resources / "PLATFORM-NOTES.txt")
    with (app / "Contents/Info.plist").open("wb") as stream:
        plistlib.dump(plist(version, minimum), stream)
    # Repository-generated MIT icon, not Apple fonts or other system assets.
    iconset = stage.parent / "EyeFlow.iconset"
    iconset.mkdir()
    for size in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            suffix = "@2x" if scale == 2 else ""
            run("sips", "-z", size * scale, size * scale, ROOT / "assets/icon-preview.png", "--out", iconset / f"icon_{size}x{size}{suffix}.png")
    run("iconutil", "-c", "icns", iconset, "-o", resources / "EyeFlow.icns")
    run("plutil", "-lint", app / "Contents/Info.plist")
    # No codesign identity, notarization, entitlement, or Gatekeeper changes.
    (stage / "Applications").symlink_to("/Applications")
    copy(ROOT / "packaging/PLATFORM-NOTES.txt", stage / "READ-ME-FIRST.txt")
    artifact = output / f"EyeFlow-{version}-macos-{arch}-unsigned.dmg"
    if os.path.lexists(artifact):
        raise ValueError(f"Refusing to overwrite {artifact}")
    run("hdiutil", "create", "-volname", "EyeFlow (unsigned preview)", "-srcfolder", stage, "-format", "UDZO", artifact)
    run("hdiutil", "verify", artifact)
    return artifact


def publish(artifact, output):
    """Atomically install each complete file without ever replacing a directory entry.

    Build artifacts must be on the output filesystem. If a concurrent writer
    claims the checksum name after the artifact link succeeds, fail and retain
    the valid artifact; never delete a potentially replaced destination.
    """
    destination = output / artifact.name
    checksum_name = artifact.name + ".sha256"
    # Anchor directory operations so replacement of the output path cannot
    # redirect either final write. lstat includes dangling symbolic links.
    directory = os.open(output, os.O_RDONLY | os.O_DIRECTORY)
    try:
        for name in (artifact.name, checksum_name):
            try:
                os.stat(name, dir_fd=directory, follow_symlinks=False)
            except FileNotFoundError:
                pass
            else:
                raise FileExistsError(f"Refusing to overwrite {output / name}")
        checksum = artifact.parent / checksum_name
        with checksum.open("xb") as stream:
            stream.write((hashlib.sha256(artifact.read_bytes()).hexdigest()
                          + "  " + artifact.name + "\n").encode("utf-8"))
        artifact.chmod(0o644)
        checksum.chmod(0o644)
        # link() is atomic and fails with EEXIST for files AND symlinks, even
        # if one appears after the preflight checks. No rename/replace fallback.
        os.link(artifact, artifact.name, dst_dir_fd=directory, follow_symlinks=False)
        os.link(checksum, checksum_name, dst_dir_fd=directory, follow_symlinks=False)
    finally:
        os.close(directory)
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", nargs="?", choices=("linux", "macos"))
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--version")
    parser.add_argument("--check-notices", action="store_true", help="Validate all four audited target notices without building")
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    parser.add_argument("--arch", choices=("arm64", "x86_64"))
    parser.add_argument("--macos-min-version")
    args = parser.parse_args()
    if args.check_notices:
        for target in ("x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-unknown-linux-gnu"):
            notice_for(target)
        print("All four target notices match audited lockfile and dependency/features configuration.")
        return
    if not args.platform or not args.binary or not args.version:
        parser.error("Packaging requires platform, --binary and --version")
    version_parts(args.version)
    actual = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if args.version != actual:
        parser.error(f"Version must match Cargo.toml ({actual}); never relabel a test build")
    if args.platform == "macos" and (not args.arch or not args.macos_min_version):
        parser.error("macOS requires --arch and --macos-min-version matching the compiled binary")
    binary = args.binary.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".eyeflow-package-", dir=output) as tmp:
        stage = Path(tmp) / "stage"
        stage.mkdir()
        private_output = Path(tmp) / "artifacts"
        private_output.mkdir()
        if args.platform == "linux":
            artifact = linux(stage, binary, args.version, private_output)
        else:
            artifact = mac(stage, binary, args.version, private_output, args.arch, args.macos_min_version)
        artifact = publish(artifact, output)
    print(artifact)
    print("Packaging only: no GUI validation, signing, notarization, or publication performed.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
