import Foundation
import ManagedSettings

/// Cleanup only. This version has no code path that applies an app shield.
/// Only our named store is cleared; other apps' restrictions are untouched.
enum BreakShield {
    static let store = ManagedSettingsStore(named: ManagedSettingsStore.Name("eyeflow.break"))

    static func clear() {
        store.clearAllSettings()
        ActivityStorage.defaults?.set(false, forKey: "shieldActive")
    }
}
