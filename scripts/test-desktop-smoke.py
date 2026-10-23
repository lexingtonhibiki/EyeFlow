"""Native EyeFlow GUI smoke, without production test hooks or OS permission changes.

Linux: this script starts its own dedicated Xvfb + Openbox session.
  python3 scripts/test-desktop-smoke.py target/debug/eyeflow
macOS: requires an existing GUI session and the official Swift toolchain.
  python3 scripts/test-desktop-smoke.py target/debug/eyeflow

Default macOS mode checks visible startup via public CGWindowListCopyWindowInfo.
With --stdin-control, a separate test-feature binary uses private stdin to invoke
the shared menu actions and verify ordinary popup shutdown, not mouse clicks.
SIGTERM is cleanup only and never a successful normal-quit observation.
No Accessibility, Screen Recording, System Events, or synthetic macOS input.
"""
import argparse
import json
import math
import select
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


class SmokeFailure(Exception):
    pass


def command(args, *, check=True, timeout=5):
    result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    if check and result.returncode:
        raise SmokeFailure(f"Command failed ({result.returncode}): {args!r}\n{result.stderr}")
    return result


def wait_for(proc, callback, timeout):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if proc.poll() is not None:
            raise SmokeFailure(f"EyeFlow exited early: {proc.returncode}")
        value = callback()
        if value:
            return value
        time.sleep(0.1)
    raise SmokeFailure(f"GUI condition timed out after {timeout}s")


def linux_windows(pid, *, only_visible=True):
    visible = ["--onlyvisible"] if only_visible else []
    ids = command(["xdotool", "search", *visible, "--pid", str(pid)], check=False)
    if ids.returncode not in (0, 1):
        raise SmokeFailure(f"Cannot enumerate X11 windows: {ids.stderr}")
    found = []
    for wid in ids.stdout.split():
        # Revalidate ownership immediately before reading or acting on a window.
        owner = command(["xdotool", "getwindowpid", wid], check=False)
        if owner.returncode or owner.stdout.strip() != str(pid):
            continue
        title = command(["xdotool", "getwindowname", wid], check=False)
        if not title.returncode:
            found.append({"id": wid, "title": title.stdout.strip(), "pid": pid})
    return found


def linux_smoke(proc, args, report, started):
    root = wait_for(proc, lambda: next((w for w in linux_windows(proc.pid)
                                       if w["title"] == "EyeFlow"), None), args.timeout)
    wait_for(proc, lambda: any(w["title"] == "EyeFlow Settings"
                              for w in linux_windows(proc.pid)), args.timeout)
    report["root_and_settings_visible"] = True
    # This is an explicitly isolated X server, and activation is PID checked.
    command(["xdotool", "windowactivate", "--sync", root["id"]])
    active = command(["xdotool", "getactivewindow"]).stdout.strip()
    if active != root["id"] or not any(w["id"] == active for w in linux_windows(proc.pid)):
        raise SmokeFailure("Refusing input: active window is not the owned EyeFlow root")
    # DEMO opens Settings but can create HeadsUp after 60 - 45 = 15 seconds.
    # Require a fresh no-popup baseline and complete hotkey delivery by 10 seconds
    # since Popen, conservatively earlier than the application's own timer start.
    if any(w["title"].startswith("EyeFlow Reminder ") for w in linux_windows(proc.pid)):
        raise SmokeFailure("Reminder already exists before hotkey; cannot attribute delivery")
    if time.monotonic() - started >= 10:
        raise SmokeFailure("Startup exceeded the 10s manual-popup attribution window")
    # XTEST triggers the existing real application global shortcut, no test hook.
    command(["xdotool", "key", "--clearmodifiers", "ctrl+shift+e"])

    def simultaneous():
        windows = linux_windows(proc.pid)
        if (any(w["title"] == "EyeFlow" for w in windows)
                and any(w["title"] == "EyeFlow Settings" for w in windows)
                and any(w["title"].startswith("EyeFlow Reminder ") for w in windows)):
            return windows
        return None

    report["simultaneous_windows"] = wait_for(proc, simultaneous, min(args.timeout, 10 - (time.monotonic() - started)))
    report["popup_after_launch_seconds"] = time.monotonic() - started
    if report["popup_after_launch_seconds"] >= 10:
        raise SmokeFailure("Popup arrived outside the manual-only attribution window")
    # EWMH close asks the WM to deliver WM_DELETE_WINDOW, unlike killing a client.
    if not any(w["id"] == root["id"] for w in linux_windows(proc.pid)):
        raise SmokeFailure("Owned root disappeared before close")
    start = time.monotonic()
    command(["wmctrl", "-ic", hex(int(root["id"]))])
    try:
        exit_code = proc.wait(timeout=max(0.01, args.max_exit_seconds - (time.monotonic() - start)))
    except subprocess.TimeoutExpired as exc:
        raise SmokeFailure("Normal root-close exit timed out; cleanup is not a pass") from exc
    elapsed = time.monotonic() - start
    if exit_code != 0 or elapsed > args.max_exit_seconds:
        raise SmokeFailure(f"Root close: exit={exit_code}, elapsed={elapsed:.3f}s")
    if linux_windows(proc.pid, only_visible=False):
        raise SmokeFailure("Owned windows remain after process exit")
    report.update(normal_root_close_exit=True, exit_code=exit_code, exit_seconds=elapsed)


def cleanup_owned(proc):
    if proc.poll() is None:
        try:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=5)
        except ProcessLookupError:
            pass


def start_private_x11(temp, children, env):
    read_fd, write_fd = os.pipe()
    try:
        with (temp / "xvfb.log").open("w") as log:
            xvfb = subprocess.Popen(["Xvfb", "-displayfd", str(write_fd), "-screen", "0",
                                     "1280x1024x24", "-nolisten", "tcp"],
                                    pass_fds=(write_fd,), stdout=log, stderr=log)
        children.append(xvfb)
    except Exception:
        os.close(read_fd)
        raise
    finally:
        os.close(write_fd)
    try:
        if not select.select([read_fd], [], [], 10)[0]:
            raise SmokeFailure("Owned Xvfb did not allocate a display within 10s")
        display = os.read(read_fd, 64).decode("ascii").strip()
        if not display.isdecimal() or xvfb.poll() is not None:
            raise SmokeFailure("Owned Xvfb failed to allocate a valid display")
    finally:
        os.close(read_fd)
    os.environ["DISPLAY"] = ":" + display
    os.environ.pop("XAUTHORITY", None)
    env["DISPLAY"] = ":" + display
    env.pop("XAUTHORITY", None)
    with (temp / "openbox.log").open("w") as log:
        wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
    children.append(wm)
    wait_for(wm, lambda: command(["wmctrl", "-m"], check=False).returncode == 0, 10)
    return ":" + display


def mac_probe(temp):
    # Compile once, then only query window metadata. Do not take screenshots.
    source = temp / "window-probe.swift"
    binary = temp / "window-probe"
    source.write_text('''import Foundation
import CoreGraphics
let pid = Int(CommandLine.arguments[1])!
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
let owned = windows.filter { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }
let data = try JSONSerialization.data(withJSONObject: owned, options: [.sortedKeys])
print(String(data: data, encoding: .utf8)!)
''', encoding="utf-8")
    command(["swiftc", str(source), "-o", str(binary)], timeout=90)
    return binary


def mac_windows(probe, pid):
    windows = json.loads(command([str(probe), str(pid)]).stdout)
    return [w for w in windows if w.get("kCGWindowBounds", {}).get("Width", 0) > 100
            and w.get("kCGWindowBounds", {}).get("Height", 0) > 100]



def mac_controlled_smoke(proc, probe, args, report):
    def send(value):
        if proc.stdin is None:
            raise SmokeFailure("Missing private control pipe")
        proc.stdin.write((value + "\n").encode("ascii"))
        proc.stdin.flush()

    wait_for(proc, lambda: mac_windows(probe, proc.pid), args.timeout)
    send("open-settings")
    before = wait_for(proc, lambda: (w if len(w := mac_windows(probe, proc.pid)) >= 2 else None), args.timeout)
    before_ids = {w["kCGWindowNumber"] for w in before}
    send("rest-now")

    def popup_created():
        windows = mac_windows(probe, proc.pid)
        return windows if len(windows) >= 3 and any(w["kCGWindowNumber"] not in before_ids for w in windows) else None

    report["simultaneous_windows"] = wait_for(proc, popup_created, args.timeout)
    started = time.monotonic()
    send("quit")
    try:
        code = proc.wait(timeout=args.max_exit_seconds)
    except subprocess.TimeoutExpired as exc:
        raise SmokeFailure("Shared menu Quit timed out; forced cleanup is not a pass") from exc
    elapsed = time.monotonic() - started
    if code != 0 or elapsed > args.max_exit_seconds:
        raise SmokeFailure(f"Shared menu Quit: exit={code}, elapsed={elapsed:.3f}s")
    deadline = time.monotonic() + 2
    while mac_windows(probe, proc.pid):
        if time.monotonic() >= deadline:
            raise SmokeFailure("Owned visible windows remain after normal quit")
        time.sleep(0.05)
    report.update(shared_menu_quit_exit=True, exit_code=code, exit_seconds=elapsed,
                  actual_tray_mouse_click="not tested; test-only private pipe supplies menu actions")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--stdin-control", action="store_true",
                        help="macOS only: test-feature binary with a private stdin command pipe")
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--max-exit-seconds", type=float, default=2)
    args = parser.parse_args()
    if not all(math.isfinite(t) and t > 0 for t in (args.timeout, args.max_exit_seconds)):
        parser.error("Timeouts must be positive and finite")
    system = sys.platform
    if system not in ("linux", "darwin"):
        parser.error("Only Linux X11 and macOS are supported by this script")
    if args.stdin_control and system != "darwin":
        parser.error("Private control lifecycle probe is macOS-only")
    if not args.binary.is_file():
        parser.error("Binary does not exist")
    if system == "linux":
        for tool in ("Xvfb", "openbox", "xdotool", "wmctrl"):
            if not shutil.which(tool):
                parser.error(f"Required test tool missing: {tool}")
    report = {"platform": system, "status": "failed",
              "scope": "x11-manual-ui-and-normal-root-close" if system == "linux" else "macos-visible-startup-only",
              "automatic_monitoring": "not validated; unknown lock state may pause it",
              "hardware_audio": "not tested (disabled)",
              "tray_panel": "not tested"}
    if args.stdin_control:
        report["scope"] = "macos-test-build-shared-menu-lifecycle"
    proc = None
    desktop_children = []
    with tempfile.TemporaryDirectory(prefix="eyeflow-native-smoke-") as temp_name:
        temp = Path(temp_name)
        home = temp / "home"
        home.mkdir()
        appdata, xdg = temp / "appdata", temp / "xdg"
        config = (home / "Library/Application Support/eyeflow" if system == "darwin"
                  else xdg / "eyeflow")
        config.mkdir(parents=True)
        (config / "config.toml").write_text(
            'language = "en-US"\nsound_enabled = false\nupdate_check_enabled = false\n'
            'hotkey_enabled = true\nshort_break_secs = 120\nstrict_mode = false\n', encoding="utf-8")
        env = os.environ.copy()
        for key in list(env):
            if key.startswith("EYEFLOW_"):
                del env[key]
        env.update(HOME=str(home), APPDATA=str(appdata), XDG_CONFIG_HOME=str(xdg),
                   XDG_CACHE_HOME=str(temp / "cache"), XDG_DATA_HOME=str(temp / "data"),
                   EYEFLOW_DEMO="1", RUST_LOG="debug")
        if args.stdin_control:
            env.pop("EYEFLOW_DEMO", None)
        if system == "linux":
            env.pop("WAYLAND_DISPLAY", None)
            env["XDG_SESSION_TYPE"] = "x11"
        output = temp / "process.log"
        try:
            if system == "linux":
                env["DISPLAY"] = start_private_x11(temp, desktop_children, env)
                env.pop("XAUTHORITY", None)
                report["display_isolation"] = "script-owned Xvfb and Openbox"
            probe = mac_probe(temp) if system == "darwin" else None
            with output.open("w", encoding="utf-8") as stream:
                started = time.monotonic()
                arguments = [str(args.binary.resolve())] + (["--smoke-stdin"] if args.stdin_control else [])
                proc = subprocess.Popen(arguments, env=env,
                                        stdin=subprocess.PIPE if args.stdin_control else subprocess.DEVNULL,
                                        cwd=temp, stdout=stream, stderr=subprocess.STDOUT)
                if system == "linux":
                    linux_smoke(proc, args, report, started)
                elif args.stdin_control:
                    mac_controlled_smoke(proc, probe, args, report)
                else:
                    report["visible_owned_windows"] = wait_for(
                        proc, lambda: (w if len(w := mac_windows(probe, proc.pid)) >= 2 else None), args.timeout)
                    report["interactive_and_normal_exit"] = "not tested; no input/permission prompts used"
                report["status"] = "passed"
        except Exception as exc:
            report["error"] = f"{type(exc).__name__}: {exc}"
        finally:
            # Never pkill/taskkill by name. Cleanup applies only to our Popen.
            if proc is not None and proc.poll() is None:
                report["cleanup_termination"] = True
                try:
                    proc.terminate()
                    try:
                        proc.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        proc.wait(timeout=5)
                except ProcessLookupError:
                    # It exited between poll() and terminate(); preserve the result.
                    pass
                except Exception as exc:
                    report["cleanup_error"] = f"{type(exc).__name__}: {exc}"
                    report["status"] = "failed"
            if proc is not None and proc.stdin is not None:
                try:
                    proc.stdin.close()
                except OSError as exc:
                    report["control_pipe_cleanup_error"] = str(exc)
                    report["status"] = "failed"
            for child in reversed(desktop_children):
                try:
                    cleanup_owned(child)
                except Exception as exc:
                    report["desktop_cleanup_error"] = f"{type(exc).__name__}: {exc}"
                    report["status"] = "failed"
            for label, path in (("process", output), ("application", config / "eyeflow.log"),
                                ("xvfb", temp / "xvfb.log"), ("openbox", temp / "openbox.log")):
                if path.exists():
                    print(f"--- {label} log ---\n{path.read_text(encoding='utf-8', errors='replace')}", file=sys.stderr)
            print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
