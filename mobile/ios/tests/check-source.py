#!/usr/bin/env python3
"""Static safety/wiring checks; not Swift compilation or device behavior tests."""
from pathlib import Path
import plistlib
import re

root = Path(__file__).resolve().parents[1]
model = (root / 'EyeFlow/EyeFlowModel.swift').read_text()
app = (root / 'EyeFlow/EyeFlowApp.swift').read_text()
view = (root / 'EyeFlow/ContentView.swift').read_text()
usage = (root / 'EyeFlow/AppUsageModel.swift').read_text()
monitor = (root / 'MonitorExtension/UsageMonitor.swift').read_text()
cleanup = (root / 'Shared/LegacyUsageCleanup.swift').read_text()
shield = (root / 'Shared/BreakShield.swift').read_text()
swift = '\n'.join(p.read_text() for p in root.rglob('*.swift'))

# No cumulative monitor, Screen Time enrollment, automatic shields or notifications.
for forbidden in ['UNNotificationRequest(', 'UNTimeIntervalNotificationTrigger(',
                  '.startMonitoring(', 'DeviceActivityEvent(', 'DeviceActivitySchedule(',
                  'requestAuthorization(', '.familyActivityPicker(', 'FamilyActivitySelection(',
                  'applyIfOptedIn', 'setShieldEnabled']:
    assert forbidden not in swift, forbidden
assert not re.search(r'\.shield\.\w+\s*=', swift)
assert '选择要关注的应用' not in view
assert '跨应用连续提醒暂不可用' in view
assert '每日累计阈值' not in usage or '不以每日累计阈值代替' in usage
assert 'Button(' not in usage and 'Toggle(' not in usage and 'Stepper(' not in usage
assert '30 秒' in usage and '15 分钟' in usage
assert '离开 EyeFlow 不能视为已经锁屏休息' in usage

# Existing prototype state is actively retired, not merely hidden in the UI.
assert 'init() {\n        LegacyUsageCleanup.disable()' in app
assert 'if phase == .active { LegacyUsageCleanup.disable() }' in app
assert 'set(false, forKey: "monitoringEnabled")' in cleanup
assert 'set(false, forKey: "shieldOptIn")' in cleanup
assert 'stopMonitoring([DeviceActivityName(ActivityStorage.activity)])' in cleanup
assert 'BreakShield.clear()' in cleanup and 'removeObject(forKey: key)' in cleanup
assert 'ActivityStorage.pendingKey' in cleanup and '"selection"' in cleanup
assert 'ManagedSettingsStore.Name("eyeflow.break")' in shield
assert 'store.clearAllSettings()' in shield
assert monitor.count('override func ') == 6
assert monitor.count('LegacyUsageCleanup.disable()') == 6
assert 'set(true' not in monitor and 'ActivityStorage.pendingKey' not in monitor

# Voluntary own-app clock remains lifecycle-gated, with snapshot persistence.
assert 'guard active else { return }' in model
assert 'timer?.invalidate()' in model and 'previousUptime = value ?' in model
assert 'eyeflow_export_state' in model and 'eyeflow_import_state' in model
app_ent = plistlib.loads((root / 'EyeFlow/EyeFlow.entitlements').read_bytes())
ext_ent = plistlib.loads((root / 'MonitorExtension/MonitorExtension.entitlements').read_bytes())
assert app_ent == ext_ent
assert app_ent['com.apple.security.application-groups'] == ['group.com.eyeflow.mobile']
info = plistlib.loads((root / 'MonitorExtension/Info.plist').read_bytes())
assert info['NSExtension']['NSExtensionPointIdentifier'] == 'com.apple.deviceactivity.monitor-extension'
print('PASS: cumulative entry/registration/shielding disabled; legacy cleanup and foreground wiring verified statically')
