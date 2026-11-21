"""Owned Windows GUI acceptance; separate desktop-smoke binary, real mouse/key input.

Geometry is observed from test-only UI logs; values are changed only via actual
widgets. Uses isolated APPDATA, no real user configuration, no sound claim.
"""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import tomllib


def parse_state(text, pid):
    hits = re.findall(
        r'GUI_STATE pid=' + str(pid) +
        r' tab=(\w+) short=(\d+) long=(\d+) theme=(\w+) dark_mode=(true|false)'
        r' quiet_editor=(\w+) quiet_periods=\[([^\r\n\]]*)\]'
        r' save_failed=(true|false) saved_feedback=(true|false)'
        r' failure_visible=(true|false) retry_visible=(true|false)',
        text,
    )
    if not hits:
        return None
    tab, short, long, theme, dark_mode, editor, periods, failed, saved, banner, retry = hits[-1]
    return {'tab': tab, 'short': int(short), 'long': int(long),
            'theme': theme, 'dark_mode': dark_mode == 'true',
            'quiet_editor': editor, 'quiet_periods': periods.split(',') if periods else [],
            'save_failed': failed == 'true', 'saved_feedback': saved == 'true',
            'failure_visible': banner == 'true', 'retry_visible': retry == 'true'}


def parse_runtime(text, pid):
    hits = re.findall(
        r'GUI_QUIET_RUNTIME pid=' + str(pid) +
        r' phase=(visible|closed) periods=\[([^\r\n\]]*)\]',
        text,
    )
    if not hits:
        return None
    phase, periods = hits[-1]
    return {'phase': phase, 'periods': periods.split(',') if periods else []}


def parse_coordinates(text, pid, name, periods=None):
    phase = '' if periods is None else ' periods=' + str(periods)
    hits = re.findall(
        r'GUI_ACCEPT ' + re.escape(name) +
        r' ([\d.]+) ([\d.]+) pid=' + str(pid) + phase + r'(?=\s|$)',
        text,
    )
    return hits[-1] if hits else None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('binary', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument('--quiet-periods', action='store_true',
                       help='Exercise only quiet periods through actual Windows widgets in Chinese')
    modes.add_argument('--quiet-save-failures', action='store_true',
                       help='Exercise native Retry and close/reopen with isolated failing profiles')
    args = parser.parse_args()
    if args.quiet_save_failures:
        errors = []
        for scenario in ('retry', 'close-reopen'):
            try:
                run_windows(args, args.output / scenario, scenario)
            except Exception as error:
                errors.append((scenario, repr(error)))
        if errors:
            raise AssertionError(f'Quiet save acceptance failed: {errors}')
    else:
        run_windows(args, args.output)


def run_windows(args, output, save_scenario=None):
    settings_title = 'EyeFlow 设置' if args.quiet_periods or save_scenario else 'EyeFlow Settings'
    assert os.name == 'nt', 'Requires an isolated Windows desktop runner'
    import pyautogui as gui
    user = ctypes.WinDLL('user32', use_last_error=True)
    user.SetProcessDPIAware()
    user.GetForegroundWindow.restype = wintypes.HWND
    user.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user.SetForegroundWindow.argtypes = [wintypes.HWND]
    user.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    user.IsWindowVisible.argtypes = [wintypes.HWND]
    user.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
    user.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
    user.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    user.PostMessageW.restype = wintypes.BOOL
    callback = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    user.EnumWindows.argtypes = [callback, wintypes.LPARAM]
    output.mkdir(parents=True, exist_ok=True)
    report = {'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              'mode': 'desktop-smoke geometry only; native mouse/key widget edits',
              'audio': 'disabled; no acoustic acceptance',
              'scenario': save_scenario, 'cases': []}
    proc = None
    launch_count = 0

    def wait(fn, seconds=20):
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            value = fn()
            if value:
                return value
            if proc is not None and proc.poll() is not None:
                raise AssertionError(f'Unexpected process exit {proc.returncode}')
            time.sleep(.1)
        raise AssertionError('Timed out waiting for GUI/persistence condition')

    def owned(hwnd):
        pid = wintypes.DWORD()
        user.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        return pid.value == proc.pid

    def settings():
        found = []
        @callback
        def visit(hwnd, _):
            title = ctypes.create_unicode_buffer(256)
            user.GetWindowTextW(hwnd, title, len(title))
            if owned(hwnd) and user.IsWindowVisible(hwnd) and title.value == settings_title:
                found.append(hwnd)
            return True
        user.EnumWindows(visit, 0)
        return found[0] if found else None

    with tempfile.TemporaryDirectory(prefix='eyeflow-rest-acceptance-') as temp:
        config = Path(temp) / 'eyeflow'
        config.mkdir()
        path = config / 'config.toml'
        fixture = (Path(__file__).parent / 'fixtures/desktop-smoke.toml').read_text()
        if args.quiet_periods or save_scenario:
            fixture = fixture.replace('language = "en-US"', 'language = "zh-CN"', 1)
        if save_scenario:
            assert fixture.count('quiet_periods = []') == 1
            fixture = fixture.replace('quiet_periods = []',
                                      'quiet_periods = [{ start = "03:05", end = "03:15" }]', 1)
        path.write_text(fixture + '\nlong_break_secs = 125\nlong_break_enabled = true\nstart_cue_enabled = false\n', encoding='utf-8')
        env = os.environ.copy()
        env.update(APPDATA=temp, RUST_LOG='info')
        log = config / 'eyeflow.log'

        def values():
            try:
                return tomllib.loads(path.read_text(encoding='utf-8'))
            except (OSError, ValueError):
                return {}

        def read_log():
            return log.read_text(encoding='utf-8', errors='replace') if log.exists() else ''

        def state(log_from=0):
            return parse_state(read_log()[log_from:], proc.pid)

        def runtime(log_from=0):
            return parse_runtime(read_log()[log_from:], proc.pid)

        def archive():
            if log.exists():
                (output / f'application-{launch_count:02d}-pid-{proc.pid}.log').write_bytes(log.read_bytes())

        def launch():
            nonlocal proc, launch_count
            launch_count += 1
            proc = subprocess.Popen([str(args.binary.resolve()), '--smoke-stdin'], env=env, stdin=subprocess.PIPE)
            proc.stdin.write(b'open-settings\n')
            proc.stdin.flush()
            hwnd = wait(settings)
            user.SetForegroundWindow(hwnd)
            wait(lambda: user.GetForegroundWindow() == hwnd)
            wait(lambda: state() is not None)
            time.sleep(.7)
            return hwnd

        def click(name, double=False, periods=None, log_from=0):
            hwnd = wait(settings)
            assert owned(hwnd)
            user.SetForegroundWindow(hwnd)
            wait(lambda: user.GetForegroundWindow() == hwnd)
            x, y = wait(lambda: parse_coordinates(read_log()[log_from:], proc.pid, name, periods))
            point = wintypes.POINT(0, 0)
            assert user.ClientToScreen(hwnd, ctypes.byref(point))
            screen_x, screen_y = point.x + float(x), point.y + float(y)
            report.setdefault('actions', []).append({'pid': proc.pid, 'widget': name, 'x': screen_x, 'y': screen_y, 'double': double})
            # Make down and up observable on separate application frames. A zero-
            # duration OS click can focus the widget without completing its click.
            gui.moveTo(screen_x, screen_y, duration=.15)
            for _ in range(2 if double else 1):
                assert user.GetForegroundWindow() == hwnd and owned(hwnd)
                gui.mouseDown()
                time.sleep(.12)
                gui.mouseUp()
                time.sleep(.12)
            time.sleep(.3)
            expected_tab = {'tab.now': 'Now', 'tab.reminder': 'Reminder',
                            'status.quiet_hours': 'Reminder'}.get(name)
            if expected_tab:
                wait(lambda: state() is not None and state()['tab'] == expected_tab)

        def screenshot(name):
            hwnd = wait(settings)
            assert owned(hwnd) and user.GetForegroundWindow() == hwnd
            rect = wintypes.RECT()
            assert user.GetWindowRect(hwnd, ctypes.byref(rect))
            gui.screenshot(region=(rect.left, rect.top, rect.right-rect.left, rect.bottom-rect.top)).save(output / (name + '.png'))

        def capture_rest_panel(name, expected_dark_mode):
            click('tab.now')
            click('status.rest_now')
            def panel():
                found = []
                @callback
                def visit(hwnd, _):
                    title = ctypes.create_unicode_buffer(256)
                    user.GetWindowTextW(hwnd, title, len(title))
                    if owned(hwnd) and user.IsWindowVisible(hwnd) and title.value.startswith('EyeFlow Reminder '):
                        found.append(hwnd)
                    return True
                user.EnumWindows(visit, 0)
                return found[0] if found else None
            hwnd = wait(panel)
            def effective():
                text = log.read_text(encoding='utf-8', errors='replace')
                hits = re.findall(r'GUI_PANEL pid=' + str(proc.pid) + r' theme=(\w+) dark_mode=(true|false)', text)
                return hits and (expected_dark_mode is None or (hits[-1][1] == 'true') is expected_dark_mode)
            wait(effective)
            time.sleep(.5)
            rect = wintypes.RECT()
            assert owned(hwnd) and user.GetWindowRect(hwnd, ctypes.byref(rect))
            gui.screenshot(region=(rect.left, rect.top, rect.right-rect.left, rect.bottom-rect.top)).save(output / (name + '-actual-rest-window.png'))
            report['cases'].append({'case': name + ' actual independent rest window', 'passed': True})

        def quit_app():
            proc.stdin.write(b'quit\n')
            proc.stdin.flush()
            proc.wait(timeout=5)
            assert proc.returncode == 0
            assert not settings(), 'Owned Settings window survived exit'
            archive()

        def edit(name, value):
            click(name, double=True)
            assert owned(user.GetForegroundWindow()), 'Refusing keyboard input to another process'
            gui.hotkey('ctrl', 'a')
            gui.write(str(value), interval=.08)
            gui.press('enter')
            time.sleep(.4)

        def choose_theme(option, expected_theme, expected_dark_mode):
            click('tab.system')
            wait(lambda: state() is not None and state()['tab'] == 'System')
            click('theme.combo')
            click('theme.option.' + option)
            wait(lambda: values().get('theme') == expected_theme.lower())

            def effective():
                current = state()
                return (current is not None and current['theme'] == expected_theme
                        and (expected_dark_mode is None
                             or current['dark_mode'] is expected_dark_mode))
            return wait(effective)

        def accept_quiet_periods():
            lunch = {'start': '12:00', 'end': '13:00'}
            after_work = {'start': '19:15', 'end': '07:45'}
            expected = [lunch, after_work]

            def check_loaded(items, editor='closed'):
                return wait(lambda: (current if (current := state())
                            and current['tab'] == 'Reminder'
                            and current['quiet_editor'] == editor
                            and current['quiet_periods'] == [
                                f"{item['start']}-{item['end']}" for item in items
                            ] else None))

            def type_time(name, value, count):
                click(name, periods=count)
                hwnd = wait(settings)
                assert owned(hwnd) and user.GetForegroundWindow() == hwnd
                gui.hotkey('ctrl', 'a')
                assert owned(hwnd) and user.GetForegroundWindow() == hwnd
                gui.write(value, interval=.1)

            click('status.quiet_hours')
            check_loaded([])
            assert values().get('quiet_periods') == []
            screenshot('09-quiet-empty')
            click('quiet.add', periods=0)
            check_loaded([], 'invalid')
            click('ui.quiet_lunch', periods=0)
            check_loaded([], 'ready')
            assert values().get('quiet_periods') == [], 'Preset must not save before Save'
            click('quiet.save', periods=0)
            check_loaded([lunch])
            wait(lambda: values().get('quiet_periods') == [lunch])

            click('quiet.add', periods=1)
            check_loaded([lunch], 'invalid')
            click('ui.quiet_after_work', periods=1)
            check_loaded([lunch], 'ready')
            type_time('quiet.start', after_work['start'], 1)
            type_time('quiet.end', after_work['end'], 1)
            screenshot('10-quiet-custom-overnight-editor')
            assert values().get('quiet_periods') == [lunch], 'Editor must not save before Save'
            click('quiet.save', periods=1)
            check_loaded(expected)
            wait(lambda: values().get('quiet_periods') == expected)
            screenshot('11-quiet-two-ranges')

            saved_pid = proc.pid
            quit_app()
            launch()
            assert proc.pid != saved_pid
            click('status.quiet_hours')
            loaded = check_loaded(expected)
            assert values().get('quiet_periods') == expected
            screenshot('12-quiet-native-after-restart')
            report['cases'].append({'case': 'two native quiet periods, including customized overnight, persist after restart',
                                    'passed': True, 'pid': proc.pid,
                                    'loaded_state': loaded})
            return loaded

        def accept_save_failure(scenario):
            old = [{'start': '03:05', 'end': '03:15'}]
            new = [*old, {'start': '19:15', 'end': '07:45'}]

            def range_strings(items):
                return [f"{item['start']}-{item['end']}" for item in items]

            def expect_state(items, *, tab='Reminder', editor='closed', failed=False, log_from=0):
                def current_matches():
                    current = state(log_from)
                    return (current if current and current['tab'] == tab
                            and current['quiet_editor'] == editor
                            and current['quiet_periods'] == range_strings(items)
                            and current['save_failed'] == failed
                            and current['failure_visible'] == failed
                            and current['retry_visible'] == failed else None)
                return wait(current_matches)

            def expect_runtime(items, *, phase='visible', log_from=0):
                return wait(lambda: runtime(log_from) ==
                            {'phase': phase, 'periods': range_strings(items)})

            def type_time(widget, value):
                click(widget, periods=1)
                hwnd = wait(settings)
                assert owned(hwnd) and user.GetForegroundWindow() == hwnd
                gui.hotkey('ctrl', 'a')
                assert owned(hwnd) and user.GetForegroundWindow() == hwnd
                gui.write(value, interval=.1)

            click('status.quiet_hours')
            expect_state(old)
            expect_runtime(old)
            assert values().get('quiet_periods') == old
            original = path.read_bytes()
            blocked = path.with_extension('toml.tmp')
            blocked.mkdir()  # Owned, empty blocker; never modify the actual config.
            failed_offset = len(read_log())
            click('quiet.add', periods=1)
            expect_state(old, editor='invalid')
            click('ui.quiet_after_work', periods=1)
            expect_state(old, editor='ready')
            type_time('quiet.start', '19:15')
            type_time('quiet.end', '07:45')
            click('quiet.save', periods=1)
            failed_state = expect_state(new, failed=True, log_from=failed_offset)
            assert not failed_state['saved_feedback']
            for widget in ('quiet.failure', 'quiet.retry'):
                wait(lambda: parse_coordinates(read_log()[failed_offset:], proc.pid, widget))
            expect_runtime(old)
            assert path.read_bytes() == original
            assert blocked.is_dir()
            screenshot('01-native-save-failure')

            if scenario == 'retry':
                blocked.rmdir()
                # Wait across the renderer refresh before clicking Retry. Until the
                # click, no background Apply or write may have made the edit live.
                time.sleep(.6)
                expect_runtime(old)
                assert path.read_bytes() == original
                expect_state(new, failed=True, log_from=failed_offset)
                recovery_offset = len(read_log())
                click('quiet.retry', log_from=failed_offset)
                applied = expect_state(new, log_from=recovery_offset)
                assert applied['saved_feedback']
                expect_runtime(new, log_from=recovery_offset)
                wait(lambda: values().get('quiet_periods') == new)
                screenshot('02-native-retry-succeeded')
                old_pid = proc.pid
                quit_app()
                launch()
                assert proc.pid != old_pid
                click('status.quiet_hours')
                expect_state(new)
                expect_runtime(new)
                assert values().get('quiet_periods') == new
                screenshot('03-native-persisted-after-restart')
            else:
                old_pid = proc.pid
                old_hwnd = wait(settings)
                assert owned(old_hwnd) and user.GetForegroundWindow() == old_hwnd
                close_offset = len(read_log())
                assert user.PostMessageW(old_hwnd, 0x0010, 0, 0)  # WM_CLOSE to owned Settings only.
                expect_runtime(old, phase='closed', log_from=close_offset)
                wait(lambda: not settings())
                assert proc.pid == old_pid and proc.poll() is None
                reopen_offset = len(read_log())
                proc.stdin.write(b'open-settings\n')
                proc.stdin.flush()
                reopened = wait(settings)
                assert owned(reopened)
                user.SetForegroundWindow(reopened)
                wait(lambda: user.GetForegroundWindow() == reopened)
                reopened_state = expect_state(old, tab='Now', log_from=reopen_offset)
                assert not reopened_state['saved_feedback']
                expect_runtime(old, log_from=reopen_offset)
                click('status.quiet_hours')
                final = expect_state(old, log_from=reopen_offset)
                assert not final['saved_feedback']
                for widget in ('quiet.failure', 'quiet.retry'):
                    assert parse_coordinates(read_log()[reopen_offset:], proc.pid, widget) is None
                assert proc.pid == old_pid and proc.poll() is None
                assert path.read_bytes() == original
                assert blocked.is_dir(), 'Reopen must discard the failed draft even while the fault persists'
                screenshot('02-native-reopened-original')
                blocked.rmdir()

            report['cases'].append({'case': scenario, 'passed': True, 'pid': proc.pid,
                                    'initial_schedule': old, 'attempted_schedule': new})

        def accept_rest_settings():
            screenshot('01-status-initial-125-seconds')
            click('tab.reminder')
            screenshot('02-reminder-125-seconds')
            click('tab.now')
            assert values()['long_break_secs'] == 125
            assert state()['long'] == 125
            quit_app()
            launch()
            assert values()['long_break_secs'] == 125
            assert state()['long'] == 125, 'New process did not load persisted 125 seconds'
            report['cases'].append({'case': '125 seconds survives no-edit tabs and restart', 'passed': True})
            for minutes, short_seconds in ((1, 599), (60, 600)):
                edit('status.long', minutes)
                wait(lambda: values().get('long_break_secs') == minutes * 60)
                edit('status.short', short_seconds)
                wait(lambda: values().get('short_break_secs') == short_seconds)
                click('tab.reminder')
                screenshot(f'03-reminder-{minutes}-minutes')
                click('tab.now')
                screenshot(f'04-status-{minutes}-minutes')
                assert values()['long_break_secs'] == minutes * 60
                quit_app()
                launch()
                cfg = values()
                assert cfg['long_break_secs'] == minutes * 60 and cfg['short_break_secs'] == short_seconds
                assert state()['long'] == minutes * 60 and state()['short'] == short_seconds, 'New process UI state differs from persisted values'
                assert cfg['sound_enabled'] is False and cfg['start_cue_enabled'] is False
                report['cases'].append({'case': f'UI edit {minutes} minutes + {short_seconds} seconds; tabs + restart', 'passed': True, 'pid': proc.pid, 'loaded_state': state()})
            screenshot('05-status-after-restart')
            for option, expected_theme, expected_dark_mode, prefix in (
                ('Light', 'Light', False, '06-light'),
                ('Dark', 'Dark', True, '07-dark'),
                ('System', 'System', None, '08-system'),
            ):
                choose_theme(option, expected_theme, expected_dark_mode)
                cfg = values()
                assert cfg['theme'] == expected_theme.lower()
                assert cfg['long_break_secs'] == 3600 and cfg['short_break_secs'] == 600
                assert cfg['sound_enabled'] is False and cfg['start_cue_enabled'] is False
                screenshot(prefix + '-system')
                click('tab.now')
                screenshot(prefix + '-status')
                click('tab.reminder')
                screenshot(prefix + '-reminder')
                capture_rest_panel(prefix, expected_dark_mode)
                quit_app()
                launch()
                wait(lambda: state() is not None and state()['theme'] == expected_theme
                     and (expected_dark_mode is None
                          or state()['dark_mode'] is expected_dark_mode))
                cfg = values()
                assert cfg['theme'] == expected_theme.lower()
                assert cfg['long_break_secs'] == 3600 and cfg['short_break_secs'] == 600
                assert cfg['sound_enabled'] is False and cfg['start_cue_enabled'] is False
                report['cases'].append({
                    'case': f'theme {expected_theme} native selection + restart',
                    'passed': True, 'pid': proc.pid, 'loaded_state': state()})
        try:
            launch()
            if save_scenario:
                accept_save_failure(save_scenario)
            elif args.quiet_periods:
                accept_quiet_periods()
            else:
                accept_rest_settings()
            quit_app()
            report['passed'] = True
        except BaseException as exc:
            report['passed'] = False
            report['error'] = repr(exc)
            if proc is not None and proc.poll() is None:
                try:
                    screenshot('failure')
                except Exception:
                    pass
            raise
        finally:
            if proc is not None and proc.poll() is None:
                proc.terminate()  # cleanup is NOT a successful quit
                proc.wait(timeout=5)
            (output / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
            archive()
            (output / 'final-config.toml').write_bytes(path.read_bytes())


if __name__ == '__main__':
    main()
