package com.eyeflow.mobile

import android.app.*
import android.content.*
import android.os.*

/** Main-thread mobile-engine ownership. Notifications are not proof of a completed rest. */
class ReminderService : Service() {
    private val handler = Handler(Looper.getMainLooper())
    private var core = 0L
    private val card by lazy { ReminderCard(this) }
    private var usage: AppUsage? = null
    private var voluntary = false
    private var delivery = ReminderDelivery()
    private var completed = 0
    private var postponed = 0
    private var natural = 0
    private var savedAt = 0L
    private val preferences by lazy { getSharedPreferences("eyeflow", MODE_PRIVATE) }
    private val notifications by lazy { getSystemService(NotificationManager::class.java) }
    private val screenEvents = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            val event = when (intent?.action) {
                Intent.ACTION_SCREEN_OFF -> EventKind.LOCK
                Intent.ACTION_USER_PRESENT -> EventKind.UNLOCK
                Intent.ACTION_SCREEN_ON -> EventKind.SCREEN_ON
                else -> null
            }
            update(event); saveSession(); refreshNotification()
        }
    }
    private val ticker = object : Runnable {
        override fun run() { if (core != 0L) { update(); handler.postDelayed(this, 1000) } }
    }
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onCreate() {
        super.onCreate()
        notifications.createNotificationChannel(NotificationChannel(SESSION, "护眼计时状态", NotificationManager.IMPORTANCE_LOW))
        // New channel ID avoids inheriting an older audible channel. User controls it thereafter.
        notifications.createNotificationChannel(NotificationChannel(PREWARN, "休息前预告（静默）", NotificationManager.IMPORTANCE_LOW).apply { setSound(null, null); enableVibration(false) })
        notifications.createNotificationChannel(NotificationChannel(QUIET, "休息到点（静默）", NotificationManager.IMPORTANCE_LOW).apply { setSound(null, null); enableVibration(false) })
        notifications.createNotificationChannel(NotificationChannel(VIBRATE, "休息到点（轻震动）", NotificationManager.IMPORTANCE_HIGH).apply { setSound(null, null); enableVibration(true); vibrationPattern=longArrayOf(0,120) })
        notifications.createNotificationChannel(NotificationChannel(SOUND, "休息到点（声音与震动）", NotificationManager.IMPORTANCE_HIGH).apply { enableVibration(true); vibrationPattern=longArrayOf(0,120) })
        notifications.createNotificationChannel(NotificationChannel(CARD_VIBRATE, "小卡片提示（轻震动）", NotificationManager.IMPORTANCE_DEFAULT).apply { setSound(null, null); enableVibration(true); vibrationPattern=longArrayOf(0,120) })
        notifications.createNotificationChannel(NotificationChannel(CARD_SOUND, "小卡片提示（声音与震动）", NotificationManager.IMPORTANCE_DEFAULT).apply { enableVibration(true); vibrationPattern=longArrayOf(0,120) })
        // Existing channels are never deleted or rewritten; Android preserves user choices.

        notifications.cancel(REST_ID); card.hide()
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_SCREEN_OFF); addAction(Intent.ACTION_SCREEN_ON); addAction(Intent.ACTION_USER_PRESENT)
            // DND broadcasts are opportunistic: on newer Android they may require
            // policy access, which we deliberately do not request. The 1 Hz ticker
            // remains the live-policy check, subject to normal OS scheduling delays.
            addAction(NotificationManager.ACTION_INTERRUPTION_FILTER_CHANGED)
            if (Build.VERSION.SDK_INT >= 28) addAction(NotificationManager.ACTION_NOTIFICATION_CHANNEL_BLOCK_STATE_CHANGED)
        }
        if (Build.VERSION.SDK_INT >= 33) registerReceiver(screenEvents, filter, RECEIVER_NOT_EXPORTED) else registerReceiver(screenEvents, filter)
    }
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == STOP) { stopSession(); return START_NOT_STICKY }
        if (core == 0L) {
            if (intent?.action != START) { stopSelf(); return START_NOT_STICKY }
            startForeground(SESSION_ID, statusNotification("正在恢复护眼会话"))
            try {
                core = NativeCore.create()
                check(core != 0L)
                preferences.getString(SNAPSHOT, null)?.let { encoded ->
                    check(NativeCore.importState(core, android.util.Base64.decode(encoded, android.util.Base64.DEFAULT))) {
                        "保存的会话无法恢复；未自动清除原记录。"
                    }
                    completed = NativeCore.completed(core)
                    postponed = NativeCore.postponed(core)
                    natural = NativeCore.natural(core)
                    delivery = ReminderDelivery(preferences.getLong("v2_alerted_id", -1), preferences.getInt("v2_alerted_phase", -1))
                }
                NativeCore.command(core, 5) // Resume preserves existing debt and pending reminders.
                usage = AppUsage(this) // Restart has no proof of foreground or lock continuity.
                handler.post(ticker)
            } catch (error: Throwable) {
                android.util.Log.e("EyeFlowSession", "Native session initialization or restoration failed", error)
                if (core != 0L) { NativeCore.destroy(core); core = 0 }
                SessionState.snapshot = SessionSnapshot(error = "计时库或已保存会话无法恢复，请检查构建；原记录未删除。")
                stopForeground(STOP_FOREGROUND_REMOVE); stopSelf(); return START_NOT_STICKY
            }
        }
        update()
        intent?.getIntExtra("command", 0)?.takeIf { it in 1..7 }?.let { command ->
            if (NativeCore.command(core, command) == 0) {
                if (command == 1) voluntary = true
                if (command == 7 || command == 4) voluntary = false
            }
            update(); saveSession()
        }
        refreshNotification()
        return START_NOT_STICKY
    }
    private fun dueChannel(): String {
        val smallCard = preferences.getBoolean("overlay_enabled", false) && android.provider.Settings.canDrawOverlays(this)
        return when(preferences.getInt("alert_mode", 0)) { 1 -> QUIET; 2 -> if (smallCard) CARD_SOUND else SOUND; else -> if (smallCard) CARD_VIBRATE else VIBRATE }
    }
    private fun channelFor(phase: Int): String = if(phase == 1 || phase == 3) PREWARN else dueChannel()
    private fun presentationPolicy(phase: Int, channel: String = channelFor(phase)): ReminderPresentationPolicy =
        ReminderPresentationPolicy(
            notificationsEnabled = notifications.areNotificationsEnabled() &&
                (preferences.contains("alert_mode") || notifications.getNotificationChannel("eyeflow_gentle_rest_v2")?.importance != NotificationManager.IMPORTANCE_NONE),
            notificationPermissionGranted = Build.VERSION.SDK_INT < 33 ||
                checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) == android.content.pm.PackageManager.PERMISSION_GRANTED,
            usagePermissionGranted = AppUsage.hasPermission(this),
            currentChannelEnabled = (notifications.getNotificationChannel(channel)?.importance ?: NotificationManager.IMPORTANCE_NONE) != NotificationManager.IMPORTANCE_NONE,
            unlocked = getSystemService(PowerManager::class.java).isInteractive &&
                !getSystemService(KeyguardManager::class.java).isKeyguardLocked,
            tracking = usage?.state == UseState.TRACKING,
            overlayOptIn = preferences.getBoolean("overlay_enabled", false),
            overlayPermissionGranted = android.provider.Settings.canDrawOverlays(this),
            interruptionsAllowed = notifications.currentInterruptionFilter == NotificationManager.INTERRUPTION_FILTER_ALL,
        )

    private fun update(screenEvent: EventKind? = null) {
        if (core == 0L) return
        val detector = usage ?: return
        // Historical intervals update accounting but never trigger retroactive delivery.
        var phase = 0
        for (interval in detector.sample(voluntary, screenEvent)) {
            phase = NativeCore.advance(core, interval.durationMs, interval.state.ordinal, false)
            if (phase < 0) { failSession(); return }
        }
        val policy = presentationPolicy(phase)
        val allowed = policy.deliveryAllowed
        phase = NativeCore.advance(core, 0, detector.state.ordinal, allowed)
        if (phase < 0) { failSession(); return }
        if (phase != 5 && phase != 6) voluntary = false
        val paused = NativeCore.paused(core) > 0
        SessionState.snapshot = SessionSnapshot(true, phase, NativeCore.remaining(core), paused,
            state = detector.state, deliveryAllowed = policy.notificationsAvailable,
            shortDebt = NativeCore.shortDebt(core), longDebt = NativeCore.longDebt(core), canPostpone = NativeCore.canPostpone(core))
        if (NativeCore.completed(core) != completed || NativeCore.postponed(core) != postponed ||
            NativeCore.natural(core) != natural || SystemClock.elapsedRealtime() - savedAt >= 15_000) {
            saveSession(); refreshNotification()
        }
        val coreCanDeliver = NativeCore.canDeliver(core)
        if (!policy.cardAllowed(coreCanDeliver)) card.hide()
        val reminderId = NativeCore.reminderId(core)
        when (delivery.decide(policy.notificationAllowed(coreCanDeliver), reminderId, phase)) {
            ReminderDelivery.Action.CANCEL -> { notifications.cancel(REST_ID); card.hide(); delivery.cancelled() }
            ReminderDelivery.Action.POST_GENTLE, ReminderDelivery.Action.POST_SILENT -> {
                val silent = delivery.decide(true, reminderId, phase) == ReminderDelivery.Action.POST_SILENT
                if (showReminder(phase, silent)) {
                    delivery.posted(reminderId, phase)
                    saveSession() // Persist marker with matching native snapshot only after successful post.
                }
            }
            ReminderDelivery.Action.NONE -> Unit
        }
    }

    private fun failSession() {
        if (core != 0L) { NativeCore.destroy(core); core = 0 }
        handler.removeCallbacks(ticker); notifications.cancel(REST_ID); card.hide()
        SessionState.snapshot = SessionSnapshot(error = "计时状态异常；已停止提醒并保留上次有效记录。")
        stopForeground(STOP_FOREGROUND_REMOVE); stopSelf()
    }
    private fun activityIntent(): PendingIntent = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    private fun serviceIntent(code: Int): PendingIntent = PendingIntent.getService(this, code, Intent(this, ReminderService::class.java).apply {
        action = if (code == 99) STOP else COMMAND; putExtra("command", code)
    }, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    private fun statusNotification(text: String): Notification = Notification.Builder(this, SESSION)
        .setSmallIcon(android.R.drawable.ic_menu_view).setContentTitle("EyeFlow · 护眼会话")
        .setContentText(text).setContentIntent(activityIntent()).setOngoing(true).setOnlyAlertOnce(true)
        .setVisibility(Notification.VISIBILITY_SECRET)
        .addAction(Notification.Action.Builder(null, "停止会话", serviceIntent(99)).build()).build()
    private fun refreshNotification() {
        if (core == 0L) return
        val message = when {
            !presentationPolicy(SessionState.snapshot.phase).notificationsAvailable -> "通知或使用权限不可用，待休息状态已保留"
            NativeCore.paused(core) > 0 -> "提醒已暂停；目标应用使用仍计时"
            usage?.state == UseState.LOCKED -> "已锁屏，不提醒；仅确认的锁屏时间计作休息"
            usage?.state == UseState.UNKNOWN -> "状态未知，保留待休息时间，不推定使用或休息"
            usage?.state == UseState.OUT_OF_SCOPE -> "其他应用不累计，也不计作休息"
            else -> "连续目标应用护眼计时；点按查看"
        }
        notifications.notify(SESSION_ID, statusNotification(message))
    }
    private fun showReminder(phase: Int, silent: Boolean): Boolean {
        val long = phase == 3 || phase == 4
        val prewarn = phase == 1 || phase == 3
        val channel = channelFor(phase)
        if (!presentationPolicy(phase, channel).notificationAllowed(NativeCore.canDeliver(core))) return false
        return try {
        val builder = Notification.Builder(this, channel)
            .setSmallIcon(android.R.drawable.ic_menu_view)
            .setContentTitle(if (prewarn) "稍后提醒你休息" else if (long) "方便时，安排一段休息" else "眼睛想歇一会儿了")
            .setContentText(if (prewarn) "还有约 ${NativeCore.remaining(core)} 秒。先忙完手上的事，由你决定何时休息。" else if (long) "建议休息一会儿。可稍后提醒，或这次先略过。" else "方便的话，放下手机看看远处；也可以稍后再休息。")
            .setContentIntent(activityIntent()).setAutoCancel(true).setOnlyAlertOnce(silent)
            .setVisibility(Notification.VISIBILITY_SECRET)

            .addAction(Notification.Action.Builder(null, "这次略过", serviceIntent(3)).build())
        if (NativeCore.canPostpone(core)) builder.addAction(Notification.Action.Builder(null, "5 分钟后", serviceIntent(2)).build())
        if (silent) {
            // API26+ group-alert suppression silences this child even when the
            // user changed the channel to audible; no summary is posted.
            builder.setGroup("eyeflow_silent_restore").setGroupSummary(false)
                .setGroupAlertBehavior(Notification.GROUP_ALERT_SUMMARY)
                .setSound(null).setVibrate(null).setDefaults(0)
        }
        val notification = builder.build()
        if (!presentationPolicy(phase, channel).notificationAllowed(NativeCore.canDeliver(core))) return false
        notifications.notify(REST_ID, notification)
        // One optional visual card only on the first due delivery, never on silent restoration.
        // Do Not Disturb suppresses the optional overlay as well as system-controlled alerts.
        if (!prewarn && !silent && presentationPolicy(phase, channel).cardAllowed(NativeCore.canDeliver(core))) {
            card.show(long, NativeCore.canPostpone(core)) { command -> startService(Intent(this, ReminderService::class.java).setAction(COMMAND).putExtra("command",command)) }
        }
        true
        } catch (_: SecurityException) { false }
    }
    private fun saveSession() {
        if (core == 0L) return
        NativeCore.exportState(core)?.let { bytes ->
            val newCompleted = NativeCore.completed(core)
            val newPostponed = NativeCore.postponed(core)
            val newNatural = NativeCore.natural(core)
            preferences.edit()
                .putInt("v2_completed", preferences.getInt("v2_completed", 0) + (newCompleted - completed).coerceAtLeast(0))
                .putInt("v2_postponed", preferences.getInt("v2_postponed", 0) + (newPostponed - postponed).coerceAtLeast(0))
                .putInt("v2_natural", preferences.getInt("v2_natural", 0) + (newNatural - natural).coerceAtLeast(0))
                .putLong("v2_alerted_id", delivery.lastAlerted.id)
                .putInt("v2_alerted_phase", delivery.lastAlerted.phase)
                .putString(SNAPSHOT, android.util.Base64.encodeToString(bytes, android.util.Base64.NO_WRAP)).apply()
            completed = newCompleted; postponed = newPostponed; natural = newNatural
            savedAt = SystemClock.elapsedRealtime()
        }
    }
    private fun stopSession() {
        if (core != 0L) update()
        if (core != 0L) { NativeCore.command(core, 6); saveSession(); NativeCore.destroy(core); core = 0 }
        handler.removeCallbacks(ticker); notifications.cancel(REST_ID); card.hide()
        SessionState.snapshot = SessionSnapshot()
        stopForeground(STOP_FOREGROUND_REMOVE); stopSelf()
    }
    override fun onConfigurationChanged(newConfig: android.content.res.Configuration) {
        super.onConfigurationChanged(newConfig); card.hide()
    }
    override fun onDestroy() {
        handler.removeCallbacks(ticker); unregisterReceiver(screenEvents)
        saveSession()
        if (core != 0L) NativeCore.destroy(core)
        core = 0; notifications.cancel(REST_ID); card.hide()
        if (SessionState.snapshot.error == null) SessionState.snapshot = SessionSnapshot()
        super.onDestroy()
    }
    companion object {
        const val START = "com.eyeflow.START"
        const val STOP = "com.eyeflow.STOP"
        const val COMMAND = "com.eyeflow.COMMAND"
        const val OBSERVE = "com.eyeflow.OBSERVE"
        const val SNAPSHOT = "mobile_snapshot_v2"
        private const val SESSION = "eyeflow_session"
        private const val PREWARN = "eyeflow_prewarn_v3"
        private const val QUIET = "eyeflow_due_quiet_v3"
        private const val VIBRATE = "eyeflow_due_vibrate_v3"
        private const val CARD_VIBRATE = "eyeflow_card_vibrate_v3"
        private const val CARD_SOUND = "eyeflow_card_sound_v3"
        private const val SOUND = "eyeflow_due_sound_v3"
        private const val SESSION_ID = 10
        private const val REST_ID = 11
    }
}
