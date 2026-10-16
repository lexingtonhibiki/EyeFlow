package com.eyeflow.mobile

fun testReminderDelivery() {
    val d = ReminderDelivery()
    check(d.decide(true, 7, 2) == ReminderDelivery.Action.POST_GENTLE)
    // A failed send is not acknowledged, so it remains eligible for retry.
    check(d.decide(true, 7, 2) == ReminderDelivery.Action.POST_GENTLE)
    d.posted(7, 2)
    check(d.decide(true, 7, 2) == ReminderDelivery.Action.NONE)
    check(d.decide(false, 7, 2) == ReminderDelivery.Action.CANCEL)
    d.cancelled()
    check(d.decide(false, 7, 2) == ReminderDelivery.Action.NONE) // Locked never restores.
    check(d.decide(true, 7, 2) == ReminderDelivery.Action.POST_SILENT)
    d.posted(7, 2)
    check(d.decide(true, 7, 2) == ReminderDelivery.Action.NONE)
    check(d.decide(true, 7, 4) == ReminderDelivery.Action.POST_GENTLE)
    d.posted(7, 4)
    check(d.decide(true, 7, 4) == ReminderDelivery.Action.NONE)
    val restored = ReminderDelivery(d.lastAlerted.id, d.lastAlerted.phase)
    check(restored.decide(true, 7, 4) == ReminderDelivery.Action.POST_SILENT)
    check(restored.decide(false, 7, 4) == ReminderDelivery.Action.NONE)
    check(restored.decide(true, 8, 4) == ReminderDelivery.Action.POST_GENTLE)
    val progressive = ReminderDelivery()
    check(progressive.decide(true, 9, 1) == ReminderDelivery.Action.POST_GENTLE)
    progressive.posted(9, 1)
    check(progressive.decide(true, 9, 1) == ReminderDelivery.Action.NONE)
    check(progressive.decide(true, 9, 2) == ReminderDelivery.Action.POST_GENTLE)
    progressive.posted(9, 2)
    repeat(100) { check(progressive.decide(true, 9, 2) == ReminderDelivery.Action.NONE) }
    check(progressive.decide(false, 9, 2) == ReminderDelivery.Action.CANCEL)
    progressive.cancelled()
    check(progressive.decide(true, 9, 2) == ReminderDelivery.Action.POST_SILENT)
    println("ReminderDelivery cancel, silent restore, locked, new-phase and restart checks passed")
}
