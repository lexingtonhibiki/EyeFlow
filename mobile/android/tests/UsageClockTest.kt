package com.eyeflow.mobile

private val chosen = setOf("reader", "video")
private fun sample(clock: UsageClock, time: Long, events: List<UseEvent> = emptyList(), reliable: Boolean = true,
                   locked: Boolean = false, own: Boolean = false, rest: Boolean = false) =
    clock.sample(time, events, reliable, locked, chosen, own, rest)
private fun List<UseInterval>.time(state: UseState) = filter { it.state == state }.sumOf { it.durationMs }
fun main() {
    testReminderDelivery()
    testReminderPresentationPolicy()
    val boundary = UsageClock(0, "eyeflow")
    sample(boundary, 0)
    check(sample(boundary, 1000, listOf(UseEvent(0, EventKind.FOREGROUND, "reader"))).time(UseState.TRACKING) == 1000L)
    check(sample(boundary, 1000).sumOf { it.durationMs } == 0L)
    check(sample(boundary, 2000).time(UseState.TRACKING) == 1000L)
    val c = UsageClock(0, "eyeflow")
    sample(c, 0)
    val switches = sample(c, 10000, listOf(
        UseEvent(1000, EventKind.FOREGROUND, "reader"),
        UseEvent(4000, EventKind.FOREGROUND, "video"),
        UseEvent(7000, EventKind.FOREGROUND, "other")))
    check(switches.time(UseState.TRACKING) == 6000L) // NOT the final foreground boolean.
    check(switches.time(UseState.OUT_OF_SCOPE) == 3000L)
    check(switches.time(UseState.LOCKED) == 0L)
    check(sample(c, 20000).time(UseState.OUT_OF_SCOPE) == 10000L) // Other app never rest.
    val revoked = sample(c, 25000, reliable = false, locked = true)
    check(revoked.time(UseState.UNKNOWN) == 5000L && revoked.time(UseState.LOCKED) == 0L)
    sample(c, 25000, locked = true)
    check(sample(c, 35000, locked = true).time(UseState.LOCKED) == 10000L)
    val gap = sample(c, 100000, locked = true)
    check(gap.time(UseState.UNKNOWN) == 65000L && gap.time(UseState.LOCKED) == 0L)
    sample(c, 100000, listOf(UseEvent(100000, EventKind.UNLOCK)))
    val explicit = sample(c, 101000, own = true, rest = false)
    check(explicit.time(UseState.VOLUNTARY_REST) == 0L)
    sample(c, 101000, own = true, rest = true)
    check(sample(c, 102000, own = true, rest = true).time(UseState.VOLUNTARY_REST) == 1000L)
    val interrupted = sample(c, 104000, listOf(UseEvent(103000, EventKind.FOREGROUND, "reader")), own = false, rest = true)
    check(interrupted.time(UseState.TRACKING) == 1000L)
    check(interrupted.time(UseState.VOLUNTARY_REST) == 0L) // Lost own-app visibility is conservative.
    val rollback = sample(c, 1)
    check(rollback.sumOf { it.durationMs } == 0L)
    val screen = UsageClock(0, "eyeflow")
    sample(screen, 0, locked = true)
    val on = sample(screen, 10000, listOf(UseEvent(5000, EventKind.SCREEN_ON)))
    check(on.time(UseState.LOCKED) == 5000L && on.time(UseState.UNKNOWN) == 5000L)
    val missingUnlock = UsageClock(0, "eyeflow")
    sample(missingUnlock, 0, locked = true)
    check(sample(missingUnlock, 10000).time(UseState.LOCKED) == 0L)
    // An unlock outside this sample cannot prove when an observed lock ended.
    // Otherwise 29s of prior lock plus this false 1s can clear a short-use burden.
    for (kind in listOf(EventKind.UNLOCK, EventKind.SCREEN_ON)) {
        for (timestamp in listOf(999L, 2001L)) {
            val staleUnlock = UsageClock(1000, "eyeflow")
            sample(staleUnlock, 1000, locked = true)
            val missingBoundary = sample(staleUnlock, 2000, listOf(UseEvent(timestamp, kind)))
            check(missingBoundary.time(UseState.LOCKED) == 0L) {
                "$kind at $timestamp outside [1000, 2000] invented locked rest"
            }
            check(missingBoundary.time(UseState.UNKNOWN) == 1000L)
        }
    }
    // A backward monotonic sample is an observation discontinuity, not a zero
    // duration continuation of the previous lock or foreground evidence.
    val reversed = UsageClock(1000, "eyeflow")
    sample(reversed, 1000, locked = true)
    val reversedSample = sample(reversed, 999, locked = true)
    check(reversedSample.all { it.state == UseState.UNKNOWN })
    check(reversed.state == UseState.UNKNOWN)
    check(sample(reversed, 2000, locked = true).time(UseState.LOCKED) == 0L)
    check(sample(reversed, 3000, locked = true).time(UseState.LOCKED) == 1000L)
    // Timestamp ties preserve event order. A terminal LOCK contradicting the
    // current unlocked sensor cannot prove rest, even if an unlock shares its time.
    for (end in listOf(EventKind.UNLOCK, EventKind.SCREEN_ON)) {
        for (boundaryAt in listOf(0L, 1000L)) {
            for (previouslyLocked in listOf(false, true)) {
                val tied = UsageClock(0, "eyeflow")
                sample(tied, 0, locked = previouslyLocked)
                val contradicted = sample(tied, 30000, listOf(
                    UseEvent(boundaryAt, end), UseEvent(boundaryAt, EventKind.LOCK)))
                check(contradicted.time(UseState.LOCKED) == 0L) {
                    "Terminal LOCK after $end at same timestamp invented rest"
                }
                check(contradicted.time(UseState.UNKNOWN) == 30000L)
                val ordered = UsageClock(0, "eyeflow")
                sample(ordered, 0, locked = previouslyLocked)
                val ended = sample(ordered, 30000, listOf(
                    UseEvent(boundaryAt, EventKind.LOCK), UseEvent(boundaryAt, end)))
                check(ended.time(UseState.LOCKED) == if (previouslyLocked) boundaryAt else 0L)
                check(ended.sumOf { it.durationMs } == 30000L)
            }
        }
    }
    val slept = UsageClock(0, "eyeflow")
    sample(slept, 0, locked = true)
    val provedSleep = sample(slept, 900000, listOf(UseEvent(900000, EventKind.SCREEN_ON), UseEvent(900000, EventKind.FOREGROUND, "reader")))
    check(provedSleep.time(UseState.LOCKED) == 900000L)
    check(slept.state == UseState.TRACKING)
    val both = UsageClock(0, "eyeflow")
    sample(both, 0)
    val historicalLock = sample(both, 950000, listOf(UseEvent(10000, EventKind.LOCK), UseEvent(920000, EventKind.SCREEN_ON)))
    check(historicalLock.time(UseState.LOCKED) == 910000L)
    check(historicalLock.time(UseState.TRACKING) == 0L)
    check(historicalLock.sumOf { it.durationMs } == 950000L)
    val missingEnd = UsageClock(0, "eyeflow")
    sample(missingEnd, 0)
    val noTerminalProof = sample(missingEnd, 900000, listOf(UseEvent(10000, EventKind.LOCK), UseEvent(900000, EventKind.FOREGROUND, "reader")))
    check(noTerminalProof.time(UseState.LOCKED) == 0L)
    check(noTerminalProof.time(UseState.UNKNOWN) == 900000L)
    val shortMissingEnd = UsageClock(0, "eyeflow")
    sample(shortMissingEnd, 0)
    check(sample(shortMissingEnd, 20000, listOf(UseEvent(1000, EventKind.LOCK), UseEvent(20000, EventKind.FOREGROUND, "reader"))).time(UseState.LOCKED) == 0L)
    val stillLocked = UsageClock(0, "eyeflow")
    sample(stillLocked, 0)
    check(sample(stillLocked, 900000, listOf(UseEvent(10000, EventKind.LOCK)), locked = true).time(UseState.LOCKED) == 890000L)
    val zeroGap = UsageClock(0, "eyeflow")
    sample(zeroGap, 0)
    val separateLocks = sample(zeroGap, 40000, listOf(
        UseEvent(1000, EventKind.LOCK), UseEvent(16000, EventKind.SCREEN_ON),
        UseEvent(16000, EventKind.LOCK), UseEvent(31000, EventKind.SCREEN_ON)))
    check(separateLocks.time(UseState.LOCKED) == 30000L)
    val lockIndexes = separateLocks.indices.filter { separateLocks[it].state == UseState.LOCKED }
    check(lockIndexes.size == 2)
    check(separateLocks.subList(lockIndexes[0] + 1, lockIndexes[1]).any { it.state == UseState.UNKNOWN })
    val restart = UsageClock(900000, "eyeflow")
    check(sample(restart, 901000).time(UseState.UNKNOWN) == 1000L)
    println("UsageClock corrected interval evidence tests passed")
}
