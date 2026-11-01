import Foundation
import DeviceActivity

/// Retire the previous cumulative-usage prototype, including persisted opt-ins.
/// Called on launch, each foreground activation, and every legacy callback.
enum LegacyUsageCleanup {
    static func disable() {
        let defaults = ActivityStorage.defaults
        // Disable intent first, then stop our registration and remove restrictions.
        defaults?.set(false, forKey: "monitoringEnabled")
        defaults?.set(false, forKey: "shieldOptIn")
        DeviceActivityCenter().stopMonitoring([DeviceActivityName(ActivityStorage.activity)])
        BreakShield.clear()
        for key in [ActivityStorage.pendingKey, "lastUsageThresholdAt", "selection", "minutes"] {
            defaults?.removeObject(forKey: key)
        }
    }
}
