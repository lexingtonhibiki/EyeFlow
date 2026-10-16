package com.eyeflow.mobile

import android.app.AppOpsManager
import android.app.KeyguardManager
import android.app.usage.UsageEvents
import android.app.usage.UsageStatsManager
import android.content.Context
import android.os.PowerManager
import android.os.Process
import android.os.SystemClock

/** Timestamped package/screen transitions only; never text or screen contents. */
internal class AppUsage(private val context: Context) {
    private var wallAnchor = System.currentTimeMillis()
    private var monoAnchor = SystemClock.elapsedRealtime()
    private val timeline = UsageClock(monoAnchor, context.packageName)
    private var selection: Set<String>? = null
    val state: UseState get() = timeline.state
    fun hasPermission(): Boolean = hasPermission(context)
    fun sample(voluntary: Boolean, screenEvent: EventKind? = null): List<UseInterval> {
        val now = SystemClock.elapsedRealtime()
        val wallNow = System.currentTimeMillis()
        val selected = context.getSharedPreferences("eyeflow", Context.MODE_PRIVATE)
            .getStringSet("selected_apps", emptySet())?.toSet() ?: emptySet()
        val permission = hasPermission()
        var reliable = permission && kotlin.math.abs((wallNow - wallAnchor) - (now - monoAnchor)) <= 2000 &&
            (selection == null || selection == selected)
        val events = mutableListOf<UseEvent>()
        if (reliable && wallNow > wallAnchor) {
            try {
                val stream = context.getSystemService(UsageStatsManager::class.java).queryEvents(wallAnchor, wallNow)
                if (stream == null) reliable = false else {
                    val event = UsageEvents.Event()
                    while (stream.hasNextEvent()) {
                        stream.getNextEvent(event)
                        val time = monoAnchor + event.timeStamp - wallAnchor
                        val kind = when (event.eventType) {
                            UsageEvents.Event.MOVE_TO_FOREGROUND -> EventKind.FOREGROUND
                            UsageEvents.Event.MOVE_TO_BACKGROUND -> EventKind.BACKGROUND
                            UsageEvents.Event.SCREEN_NON_INTERACTIVE, UsageEvents.Event.KEYGUARD_SHOWN -> EventKind.LOCK
                            UsageEvents.Event.KEYGUARD_HIDDEN -> EventKind.UNLOCK
                            UsageEvents.Event.SCREEN_INTERACTIVE -> EventKind.SCREEN_ON
                            else -> null
                        }
                        if (kind != null && event.timeStamp >= wallAnchor) events += UseEvent(time, kind, event.packageName)
                    }
                }
            } catch (_: SecurityException) { reliable = false }
        }
        if (screenEvent != null) events += UseEvent(now, screenEvent)
        val locked = !context.getSystemService(PowerManager::class.java).isInteractive ||
            context.getSystemService(KeyguardManager::class.java).isKeyguardLocked
        val result = timeline.sample(now, events, reliable, locked, selected, SessionState.ownVisible, voluntary)
        wallAnchor = wallNow; monoAnchor = now; selection = selected
        return result
    }
    companion object {
        fun hasPermission(context: Context): Boolean = context.getSystemService(AppOpsManager::class.java)
            .checkOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), context.packageName) == AppOpsManager.MODE_ALLOWED
    }
}
