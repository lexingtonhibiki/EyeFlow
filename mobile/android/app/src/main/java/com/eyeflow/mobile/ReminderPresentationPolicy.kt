package com.eyeflow.mobile

/** Live delivery permissions are separate from reminder debt and alert history. */
internal data class ReminderPresentationPolicy(
    val notificationsEnabled: Boolean,
    val notificationPermissionGranted: Boolean,
    val usagePermissionGranted: Boolean,
    val currentChannelEnabled: Boolean,
    val unlocked: Boolean,
    val tracking: Boolean,
    val overlayOptIn: Boolean,
    val overlayPermissionGranted: Boolean,
    val interruptionsAllowed: Boolean,
) {
    val notificationsAvailable: Boolean
        get() = notificationsEnabled && notificationPermissionGranted &&
            usagePermissionGranted && currentChannelEnabled

    // Supplied to the engine before asking whether a pending round can be shown.
    val deliveryAllowed: Boolean
        get() = notificationsAvailable && unlocked && tracking

    fun notificationAllowed(coreCanDeliver: Boolean): Boolean = deliveryAllowed && coreCanDeliver

    // DND does not suppress system-controlled notifications here. It only closes
    // our separately managed window, which Android notification policy cannot hide.
    fun cardAllowed(coreCanDeliver: Boolean): Boolean = notificationAllowed(coreCanDeliver) &&
        overlayOptIn && overlayPermissionGranted && interruptionsAllowed
}
