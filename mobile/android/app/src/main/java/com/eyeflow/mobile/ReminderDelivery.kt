package com.eyeflow.mobile

/** Visibility is not delivery history: restore an actionable item without re-alerting. */
internal class ReminderDelivery(initialId: Long = -1, initialPhase: Int = -1) {
    data class Key(val id: Long, val phase: Int)
    enum class Action { NONE, CANCEL, POST_GENTLE, POST_SILENT }
    var lastAlerted = Key(initialId, initialPhase)
        private set
    private var visible: Key? = null

    fun decide(eligible: Boolean, id: Long, phase: Int): Action {
        if (!eligible || id <= 0) return if (visible != null) Action.CANCEL else Action.NONE
        val key = Key(id, phase)
        if (visible == key) return Action.NONE
        return if (lastAlerted == key) Action.POST_SILENT else Action.POST_GENTLE
    }
    fun cancelled() { visible = null }
    fun posted(id: Long, phase: Int) {
        val key = Key(id, phase)
        visible = key
        lastAlerted = key
    }
}
