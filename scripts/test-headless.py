#!/usr/bin/env python3
"""Run real platform-independent modules on Linux without the Win32 shell.

Requires Python 3.11+, the project's Rust toolchain, and ALSA development/link
libraries. Run `cargo fetch --locked` first to populate dependency caches.
No audio device or graphical display is needed. This supplements,
never replaces, `cargo test --locked` and hardware acceptance on Windows.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def toml(value):
    if isinstance(value, bool):
        return str(value).lower()
    if isinstance(value, str):
        return json.dumps(value)
    if isinstance(value, list):
        return "[" + ", ".join(map(toml, value)) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{k} = {toml(v)}" for k, v in value.items()) + " }"
    raise TypeError(value)


def main():
    original = tomllib.loads((ROOT / "Cargo.toml").read_text())
    modules = ["audio", "config", "core", "stats", "tr", "i18n", "ui", "wallpaper", "update", "tips"]
    dependencies = ["egui", "image", "rodio", "serde", "toml", "log", "chrono", "rand", "ureq", "serde_json"]
    with tempfile.TemporaryDirectory(prefix="eyeflow-headless-") as directory:
        work = Path(directory)
        # Only link the public project inputs the checks use, never arbitrary
        # files such as a developer's .env, credentials, caches or local notes.
        inputs = ["src", "assets", "docs", "locales", "README.md", "CONTRIBUTING.md",
                  "CONTEXT.md", "CHANGELOG.md", "LICENSE", "build.rs", ".github",
                  "installer", "examples", "build-release.cmd", "scripts"]
        for name in inputs:
            item = ROOT / name
            if item.exists():
                (work / name).symlink_to(item, target_is_directory=item.is_dir())
        manifest = ["[package]", 'name = "eyeflow-headless-tests"',
                    f'version = {toml(original["package"]["version"])}',
                    'edition = "2021"', 'autobins = false', 'autoexamples = false', 'build = "build.rs"',
                    '[lib]', 'path = "headless.rs"', '[dependencies]']
        manifest += [f'{name} = {toml(original["dependencies"][name])}' for name in dependencies]
        manifest += ['[build-dependencies]', 'winresource = "0.1"', '[features]']
        manifest += [f'{name} = {toml(value)}' for name, value in original['features'].items()]
        (work / "Cargo.toml").write_text("\n".join(manifest) + "\n")
        (work / "headless.rs").write_text("#![allow(dead_code)]\n" + "\n".join(
            f'#[path = "src/{name}.rs"]\nmod {name};' for name in modules))
        # Retain the product's dependency selections. Cargo only adjusts the
        # temporary root-package entry; it never edits the product lockfile.
        shutil.copyfile(ROOT / "Cargo.lock", work / "Cargo.lock")
        (work / "tests").mkdir()
        for name in ["i18n_locales.rs", "i18n_tray_ids.rs", "readme_bilingual.rs"]:
            (work / "tests" / name).symlink_to(ROOT / "tests" / name)
        env = os.environ.copy()
        env["APPDATA"] = str(work / "appdata")
        env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target" / "headless"))
        subprocess.run(["cargo", "test", "--offline", "--manifest-path", str(work / "Cargo.toml")],
                       env=env, check=True)


if __name__ == "__main__":
    main()
