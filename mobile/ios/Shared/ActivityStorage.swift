import Foundation

/// Change this identifier in BOTH entitlements and this constant for your team.
enum ActivityStorage {
    static let group = "group.com.eyeflow.mobile"
    static var defaults: UserDefaults? {
        // UserDefaults(suiteName:) alone can succeed without valid App Group
        // entitlement, leaving the app and extension with nonshared storage.
        guard FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group) != nil else { return nil }
        return UserDefaults(suiteName: group)
    }
    static let activity = "eyeflow.selected-apps"
    static let event = "eyeflow.usage-threshold"
    static let pendingKey = "pendingUsageReminder"
}
