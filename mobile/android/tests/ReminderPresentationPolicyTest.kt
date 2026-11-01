package com.eyeflow.mobile

internal fun availablePresentationPolicy() = ReminderPresentationPolicy(
    notificationsEnabled = true,
    notificationPermissionGranted = true,
    usagePermissionGranted = true,
    currentChannelEnabled = true,
    unlocked = true,
    tracking = true,
    overlayOptIn = true,
    overlayPermissionGranted = true,
    interruptionsAllowed = true,
)

fun testReminderPresentationPolicy() {
    val allowed = availablePresentationPolicy()
    check(allowed.notificationAllowed(true) && allowed.cardAllowed(true))
    check(!allowed.notificationAllowed(false) && !allowed.cardAllowed(false))

    // A live DND change hides an existing card without cancelling the system
    // notification, clearing alert history, or reopening a card when DND ends.
    val delivery = ReminderDelivery()
    delivery.posted(9, 2)
    val dnd = allowed.copy(interruptionsAllowed = false)
    check(dnd.notificationAllowed(true) && !dnd.cardAllowed(true))
    check(delivery.decide(dnd.notificationAllowed(true), 9, 2) == ReminderDelivery.Action.NONE)
    check(delivery.decide(allowed.notificationAllowed(true), 9, 2) == ReminderDelivery.Action.NONE)
    check(delivery.lastAlerted == ReminderDelivery.Key(9, 2))

    for (policy in listOf(
        allowed.copy(overlayPermissionGranted = false),
        allowed.copy(overlayOptIn = false),
    )) {
        check(policy.notificationAllowed(true) && !policy.cardAllowed(true))
        check(delivery.decide(policy.notificationAllowed(true), 9, 2) == ReminderDelivery.Action.NONE)
    }

    // Permission/channel/lock changes withdraw the notification and its card,
    // preserving the marker for one silent notification-only restoration.
    for (policy in listOf(
        allowed.copy(notificationsEnabled = false),
        allowed.copy(notificationPermissionGranted = false),
        allowed.copy(usagePermissionGranted = false),
        allowed.copy(currentChannelEnabled = false),
        allowed.copy(unlocked = false),
        allowed.copy(tracking = false),
    )) {
        check(!policy.notificationAllowed(true) && !policy.cardAllowed(true))
        check(delivery.decide(policy.notificationAllowed(true), 9, 2) == ReminderDelivery.Action.CANCEL)
        delivery.cancelled()
        check(delivery.lastAlerted == ReminderDelivery.Key(9, 2))
        check(delivery.decide(allowed.notificationAllowed(true), 9, 2) == ReminderDelivery.Action.POST_SILENT)
        delivery.posted(9, 2)
    }

    // The service supplies the current phase's channel. Blocking PREWARN must
    // clear its visibility marker, but must not block a later enabled due channel.
    val phases = ReminderDelivery()
    phases.posted(12, 1)
    val blockedPrewarn = allowed.copy(currentChannelEnabled = false)
    check(phases.decide(blockedPrewarn.notificationAllowed(true), 12, 1) == ReminderDelivery.Action.CANCEL)
    phases.cancelled()
    check(phases.decide(allowed.notificationAllowed(true), 12, 1) == ReminderDelivery.Action.POST_SILENT)
    check(phases.decide(allowed.notificationAllowed(true), 12, 2) == ReminderDelivery.Action.POST_GENTLE)
    println("ReminderPresentationPolicy live DND, overlay, permission, phase-channel and lock checks passed")
}
