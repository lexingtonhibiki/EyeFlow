"""Windows native tray-command smoke test; no production test hooks.

Runs only a supplied EyeFlow test binary with isolated APPDATA. Opens its real
tray menu to discover IDs, posts the same WM_COMMAND as native menu selection,
then measures process exit with settings + a live break window visible.
A timeout is a FAILURE; termination is cleanup, never a successful exit.
"""
import argparse
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--max-exit-seconds", type=float, default=2.0)
    args = parser.parse_args()
    if os.name != "nt":
        raise SystemExit("This test requires a Windows desktop session")
    user = c.WinDLL("user32", use_last_error=True)
    callback_type = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
    user.EnumWindows.argtypes = [callback_type, w.LPARAM]
    user.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
    user.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
    user.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
    user.IsWindowVisible.argtypes = [w.HWND]
    user.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
    user.SendMessageTimeoutW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM, w.UINT, w.UINT, c.POINTER(c.c_size_t)]
    user.GetMenuItemCount.argtypes = [w.HMENU]
    user.GetMenuItemID.argtypes = [w.HMENU, c.c_int]
    user.GetMenuItemID.restype = w.UINT
    user.GetMenuStringW.argtypes = [w.HMENU, w.UINT, w.LPWSTR, c.c_int, w.UINT]

    def windows(pid):
        found = []
        @callback_type
        def visit(hwnd, _):
            owner = w.DWORD()
            user.GetWindowThreadProcessId(hwnd, c.byref(owner))
            if owner.value == pid:
                cls, title = c.create_unicode_buffer(256), c.create_unicode_buffer(512)
                user.GetClassNameW(hwnd, cls, len(cls))
                user.GetWindowTextW(hwnd, title, len(title))
                found.append((hwnd, cls.value, title.value, bool(user.IsWindowVisible(hwnd))))
            return True
        user.EnumWindows(visit, 0)
        return found

    def wait_for(fn, timeout=15):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            value = fn()
            if value:
                return value
            time.sleep(0.02)
        raise AssertionError("Windows UI did not reach expected state")

    with tempfile.TemporaryDirectory(prefix="eyeflow-quit-") as temp:
        config = Path(temp) / "eyeflow"
        config.mkdir()
        (config / "config.toml").write_text(
            'language = "en-US"\nsound_enabled = false\nupdate_check_enabled = false\n'
            'short_break_secs = 120\n', encoding="utf-8")
        env = os.environ.copy()
        env["APPDATA"] = temp
        env["RUST_LOG"] = "debug"
        proc = subprocess.Popen([str(args.binary.resolve())], env=env)
        try:
            tray = wait_for(lambda: next((x[0] for x in windows(proc.pid) if x[1] == "tray_icon_app"), None))

            def command(label):
                # tray-icon 0.24's native notification message and WM_RBUTTONUP.
                assert user.PostMessageW(tray, 6002, 0, 0x0205)
                popup = wait_for(lambda: next((x[0] for x in windows(proc.pid) if x[1] == "#32768"), None))
                menu = c.c_size_t()
                assert user.SendMessageTimeoutW(popup, 0x01E1, 0, 0, 2, 1000, c.byref(menu))
                item_id = None
                for pos in range(user.GetMenuItemCount(menu.value)):
                    text = c.create_unicode_buffer(256)
                    user.GetMenuStringW(menu.value, pos, text, len(text), 0x0400)
                    if label.lower() in text.value.replace("&", "").lower():
                        item_id = user.GetMenuItemID(menu.value, pos)
                        break
                assert item_id is not None, f"Menu item not found: {label}"
                # End tracking, then deliver the real native command to its owner.
                assert user.PostMessageW(tray, 0x001F, 0, 0)  # WM_CANCELMODE
                start = time.monotonic()
                assert user.PostMessageW(tray, 0x0111, item_id, 0)  # WM_COMMAND
                return start

            command("Settings")
            wait_for(lambda: any(x[3] and x[2] == "EyeFlow Settings" for x in windows(proc.pid)))
            command("Rest now")
            wait_for(lambda: any(x[3] and x[2].startswith("EyeFlow Reminder") for x in windows(proc.pid)))
            before = windows(proc.pid)
            assert any(x[3] and x[2] == "EyeFlow Settings" for x in before), before
            assert any(x[3] and x[2].startswith("EyeFlow Reminder") for x in before), before
            start = command("Quit")
            proc.wait(timeout=args.max_exit_seconds)
            elapsed = time.monotonic() - start
            assert proc.returncode == 0, proc.returncode
            assert not windows(proc.pid), "Native windows survived process exit"
            print(json.dumps({"exit_seconds": elapsed, "windows_before": before, "exit_code": proc.returncode}))
        finally:
            log = config / "eyeflow.log"
            if log.exists():
                print(log.read_text(encoding="utf-8", errors="replace"))
            if proc.poll() is None:
                proc.terminate()  # cleanup only; exceptions/timeouts still fail the test
                proc.wait(timeout=5)


if __name__ == "__main__":
    main()
