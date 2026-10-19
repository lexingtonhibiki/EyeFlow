package com.eyeflow.mobile

/** Evidence states, not an inferred daily usage total. Numeric values are the mobile C ABI. */
internal enum class UseState { TRACKING, OUT_OF_SCOPE, LOCKED, UNKNOWN, STOPPED, VOLUNTARY_REST }
internal enum class EventKind { FOREGROUND, BACKGROUND, LOCK, UNLOCK, SCREEN_ON }
internal data class UseEvent(val atMs: Long, val kind: EventKind, val packageName: String? = null)
internal data class UseInterval(val durationMs: Long, val state: UseState)

/** Reconstruct exclusive monotonic intervals. Unreliable gaps never become usage or rest. */
internal class UsageClock(startMs: Long, private val ownPackage: String) {
    private var cursor = startMs
    private var foreground: String? = null
    private var locked: Boolean? = null
    private var ownWasVisible = false
    var state = UseState.UNKNOWN
        private set

    fun sample(nowMs: Long, events: List<UseEvent>, reliable: Boolean, currentLocked: Boolean,
               selected: Set<String>, ownVisible: Boolean, voluntary: Boolean): List<UseInterval> {
        val now = nowMs.coerceAtLeast(cursor)
        val output = mutableListOf<UseInterval>()
        // Only boundaries inside this observation can prove its continuity.
        val history = events.filter { it.atMs in cursor..now }.sortedBy { it.atMs }
        val observationReliable = reliable && nowMs >= cursor
        val gap = now - cursor
        // A suspended handler, clock discontinuity or missing access cannot prove continuity.
        val unexplainedUnlock = locked == true && !currentLocked && history.none { it.kind == EventKind.UNLOCK || it.kind == EventKind.SCREEN_ON }
        val lastLock = history.filter { it.kind == EventKind.LOCK }.maxOfOrNull { it.atMs }
        val lastEnd = history.filter { it.kind == EventKind.UNLOCK || it.kind == EventKind.SCREEN_ON }.maxOfOrNull { it.atMs }
        val unclosedShortLock = gap <= MAX_RECONSTRUCT_GAP_MS && !currentLocked && lastLock != null && (lastEnd == null || lastEnd < lastLock)
        if (!observationReliable || unexplainedUnlock || unclosedShortLock) {
            output += UseInterval(gap, UseState.UNKNOWN)
            foreground = null; locked = null; ownWasVisible = false
        } else if (gap > MAX_RECONSTRUCT_GAP_MS) {
            // Sleep may suspend the Handler. A successful timestamp-aligned event
            // query can still prove LOCK spans with explicit start/end evidence.
            // It cannot turn an arbitrary unobserved app interval into screen use.
            val hasEnd = history.any { it.kind == EventKind.UNLOCK || it.kind == EventKind.SCREEN_ON }
            var lockStart: Long? = if (locked == true && hasEnd) cursor else null
            val proved = mutableListOf<Pair<Long, Long>>()
            foreground = null
            for (event in history) {
                when (event.kind) {
                    EventKind.LOCK -> { if (lockStart == null) lockStart = event.atMs; foreground = null }
                    EventKind.UNLOCK, EventKind.SCREEN_ON -> {
                        lockStart?.let { proved += it to event.atMs }
                        lockStart = null; foreground = null
                    }
                    EventKind.FOREGROUND -> if (now - event.atMs <= MAX_RECONSTRUCT_GAP_MS) foreground = event.packageName
                    EventKind.BACKGROUND -> if (foreground == event.packageName) foreground = null
                }
            }
            // Validate an entire lock span BEFORE emitting any credit. A target
            // event is not a substitute for a missing screen/keyguard boundary.
            if (currentLocked) lockStart?.let { proved += it to now }
            var at = cursor
            for ((index, span) in proved.withIndex()) {
                val (start, end) = span
                if (index > 0) output += UseInterval(0, UseState.UNKNOWN)
                if (start > at) output += UseInterval(start - at, UseState.UNKNOWN)
                output += UseInterval(end - start, UseState.LOCKED)
                at = end
            }
            if (at < now) output += UseInterval(now - at, UseState.UNKNOWN)
            locked = if (lockStart != null && currentLocked) true else null
            ownWasVisible = false
        } else {
            var at = cursor
            fun classify(): UseState = when {
                locked == true -> UseState.LOCKED
                locked != false -> UseState.UNKNOWN
                foreground in selected -> UseState.TRACKING
                foreground == ownPackage && ownWasVisible && ownVisible && voluntary -> UseState.VOLUNTARY_REST
                foreground != null -> UseState.OUT_OF_SCOPE
                else -> UseState.UNKNOWN
            }
            for (event in history) {
                output += UseInterval(event.atMs - at, classify())
                at = event.atMs
                when (event.kind) {
                    EventKind.FOREGROUND -> foreground = event.packageName
                    EventKind.BACKGROUND -> if (foreground == event.packageName) foreground = null
                    EventKind.LOCK -> locked = true
                    EventKind.UNLOCK -> { locked = false; foreground = null }
                    EventKind.SCREEN_ON -> { locked = null; foreground = null }
                }
            }
            output += UseInterval(now - at, classify())
        }
        cursor = now
        // A present-time sensor establishes proof only FROM NOW, never retroactively.
        if (!observationReliable) {
            foreground = null; locked = null; state = UseState.UNKNOWN
        } else {
            if (locked != currentLocked) {
                output += UseInterval(0, UseState.UNKNOWN) // Break prior lock/voluntary continuity.
                if (locked == true && !currentLocked) foreground = null
                locked = currentLocked
            }
            if (ownVisible && !currentLocked) foreground = ownPackage
            state = when {
                currentLocked -> UseState.LOCKED
                foreground in selected -> UseState.TRACKING
                foreground == ownPackage && ownVisible && voluntary -> UseState.VOLUNTARY_REST
                foreground != null -> UseState.OUT_OF_SCOPE
                else -> UseState.UNKNOWN
            }
        }
        ownWasVisible = ownVisible
        output += UseInterval(0, state)
        return output
    }
    companion object { const val MAX_RECONSTRUCT_GAP_MS = 30_000L }
}
