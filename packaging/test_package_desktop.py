"""Offline metadata/staging tests. Fixture bytes are NOT a runnable app."""
import importlib.util
import os
from pathlib import Path
import plistlib
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("package_desktop", Path(__file__).resolve().parents[1] / "scripts/package-desktop.py")
PACKAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGE)


class PackagingTests(unittest.TestCase):
    def test_stable_version(self):
        self.assertEqual(PACKAGE.version_parts("0.7.4"), ("0.7.4", "0.7.4"))

    def test_prerelease_sorts_before_stable(self):
        self.assertEqual(PACKAGE.version_parts("0.7.4-test.2"), ("0.7.4", "0.7.4~test.2"))

    def test_rejects_unsafe_or_malformed_version(self):
        for value in ("../0.7.4", "0.7.4\nDepends: evil", "0.7", "v0.7.4", "00.7.4", "0.7.4-", "0.7.4+metadata"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                PACKAGE.version_parts(value)

    def test_plist_round_trip(self):
        result = plistlib.loads(plistlib.dumps(PACKAGE.plist("0.7.4-test.2", "11.0")))
        self.assertEqual(result["CFBundleShortVersionString"], "0.7.4")
        self.assertEqual(result["EyeFlowBuildVersion"], "0.7.4-test.2")
        self.assertEqual(result["CFBundleIdentifier"], "io.github.lexingtonhibiki.eyeflow")
        self.assertEqual(result["LSMinimumSystemVersion"], "11.0")
        self.assertNotIn("LSUIElement", result)  # Visible non-Windows root remains accessible.

    def test_invalid_minimum(self):
        with self.assertRaises(ValueError):
            PACKAGE.plist("0.7.4", "latest")

    def test_linux_staging_contents_permissions_and_notices(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture = root / "fixture"
            fixture.write_bytes(b"NOT AN EXECUTABLE: metadata test fixture")
            stage = root / "stage"
            PACKAGE.stage_linux(stage, fixture, "0.7.4-test.2", "libc6 (>= 2.39), fonts-noto-cjk")
            control = (stage / "DEBIAN/control").read_text()
            self.assertIn("Version: 0.7.4~test.2\n", control)
            self.assertIn("Architecture: amd64\n", control)
            self.assertIn("Depends: libc6 (>= 2.39), fonts-noto-cjk\n", control)
            self.assertEqual((stage / "usr/bin/eyeflow").stat().st_mode & 0o777, 0o755)
            self.assertEqual((stage / "usr/share/applications/eyeflow.desktop").stat().st_mode & 0o777, 0o644)
            self.assertIn("MIT License", (stage / "usr/share/doc/eyeflow/copyright").read_text())
            self.assertIn("Wayland", (stage / "usr/share/doc/eyeflow/PLATFORM-NOTES.txt").read_text())
            self.assertFalse(list(stage.rglob("*.ttc")))
            self.assertFalse(list(stage.rglob("*.ttf")))
            self.assertEqual(sorted(p.name for p in (stage / "DEBIAN").iterdir()), ["control"])

    def test_publish_rejects_existing_files_and_symlinks(self):
        for suffix in ("", ".sha256"):
            for kind in ("file", "symlink", "dangling"):
                with self.subTest(suffix=suffix, kind=kind), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    private = root / "private"
                    private.mkdir()
                    artifact = private / "preview.deb"
                    artifact.write_bytes(b"new artifact")
                    victim = root / "user.txt"
                    victim.write_bytes(b"keep user data")
                    target = root / (artifact.name + suffix)
                    if kind == "file":
                        target.write_bytes(b"keep existing output")
                    else:
                        target.symlink_to(victim if kind == "symlink" else root / "missing")
                    with self.assertRaises(FileExistsError):
                        PACKAGE.publish(artifact, root)
                    self.assertEqual(victim.read_bytes(), b"keep user data")
                    self.assertFalse((root / "missing").exists())
                    if kind == "file":
                        self.assertEqual(target.read_bytes(), b"keep existing output")
                    else:
                        self.assertTrue(target.is_symlink())

    def test_publish_complete_then_repeat_does_not_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            private = root / "private"
            private.mkdir()
            artifact = private / "preview.deb"
            artifact.write_bytes(b"complete artifact")
            destination = PACKAGE.publish(artifact, root)
            self.assertEqual(destination.read_bytes(), b"complete artifact")
            checksum = (root / "preview.deb.sha256").read_bytes()
            with self.assertRaises(FileExistsError):
                PACKAGE.publish(artifact, root)
            self.assertEqual(destination.read_bytes(), b"complete artifact")
            self.assertEqual((root / "preview.deb.sha256").read_bytes(), checksum)

    def test_publish_racing_symlink_cannot_redirect_output(self):
        for name in ("preview.deb", "preview.deb.sha256"):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                private = root / "private"
                private.mkdir()
                artifact = private / "preview.deb"
                artifact.write_bytes(b"complete artifact")
                victim = root / "user.txt"
                victim.write_bytes(b"keep user data")
                real_link = os.link

                def racing_link(source, destination, **kwargs):
                    if destination == name:
                        (root / name).symlink_to(victim)
                    return real_link(source, destination, **kwargs)

                with patch.object(PACKAGE.os, "link", side_effect=racing_link):
                    with self.assertRaises(FileExistsError):
                        PACKAGE.publish(artifact, root)
                self.assertEqual(victim.read_bytes(), b"keep user data")
                self.assertTrue((root / name).is_symlink())
                if name.endswith("sha256"):
                    self.assertEqual((root / "preview.deb").read_bytes(), b"complete artifact")


if __name__ == "__main__":
    unittest.main()
