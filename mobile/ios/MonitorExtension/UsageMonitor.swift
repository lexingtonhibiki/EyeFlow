import DeviceActivity

/// Inert migration extension: old registrations may still deliver a callback.
/// Every callback only clears the retired prototype; none creates a reminder.
final class UsageMonitor: DeviceActivityMonitor {
    override func intervalDidStart(for activity: DeviceActivityName) {
        LegacyUsageCleanup.disable()
    }

    override func intervalDidEnd(for activity: DeviceActivityName) {
        LegacyUsageCleanup.disable()
    }

    override func eventDidReachThreshold(_ event: DeviceActivityEvent.Name, activity: DeviceActivityName) {
        LegacyUsageCleanup.disable()
    }

    override func intervalWillStartWarning(for activity: DeviceActivityName) {
        LegacyUsageCleanup.disable()
    }

    override func intervalWillEndWarning(for activity: DeviceActivityName) {
        LegacyUsageCleanup.disable()
    }

    override func eventWillReachThresholdWarning(_ event: DeviceActivityEvent.Name, activity: DeviceActivityName) {
        LegacyUsageCleanup.disable()
    }
}
