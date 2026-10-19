package com.eyeflow.mobile

import android.Manifest
import android.app.*
import android.content.*
import android.content.pm.PackageManager
import android.graphics.Color
import android.os.*
import android.view.Gravity
import android.view.View
import android.widget.*

class MainActivity : Activity() {
    private var resumed = false
    private val handler = Handler(Looper.getMainLooper())
    private lateinit var status: TextView
    private lateinit var timer: TextView
    private lateinit var stats: TextView
    private lateinit var start: Button
    private lateinit var stop: Button
    private val commands = mutableMapOf<Int, Button>()
    private lateinit var settings: LinearLayout
    private lateinit var detail: TextView
    private val refresh = object : Runnable {
        override fun run() { render(); handler.postDelayed(this, 1000) }
    }
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val scroll = ScrollView(this).apply { setBackgroundColor(Color.rgb(243, 247, 244)); clipToPadding = false }
        val body = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(dp(24), dp(24), dp(24), dp(24)) }
        scroll.addView(body)
        // Insets avoid Android 15's enforced edge-to-edge overlapping system bars.
        scroll.setOnApplyWindowInsetsListener { view, insets ->
            view.setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
            insets
        }
        var destination = body
        fun text(value: String, size: Float): TextView = TextView(this).apply {
            text = value; textSize = size; setTextColor(Color.rgb(23, 60, 48)); setPadding(0, dp(8), 0, dp(8)); destination.addView(this)
        }
        text("EyeFlow", 36f)
        text("让休息跟上你的节奏", 18f)
        status = text("尚未开始", 20f)
        timer = text("—", 48f).apply { gravity = Gravity.CENTER_HORIZONTAL }
        stats = text("", 15f)
        fun button(label: String, action: () -> Unit): Button = Button(this).apply { text = label; setOnClickListener { action() }; destination.addView(this, LinearLayout.LayoutParams(-1, -2)) }
        detail = text("先选择要关照的应用，按你的节奏开启。", 16f)
        start = button("开启护眼提醒") { requestStart() }
        listOf("休息一下" to 1, "5 分钟后提醒" to 2, "这次先略过" to 3, "暂停提醒 1 小时" to 4, "恢复提醒" to 5, "结束这次休息" to 7).forEach { (label, code) ->
            commands[code] = button(label) { sendCommand(code) }
        }
        button("提醒偏好与应用设置") { settings.visibility = if (settings.visibility == View.GONE) View.VISIBLE else View.GONE }
        settings = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; visibility = View.GONE }
        body.addView(settings)
        destination = settings
        button("1. 授权使用情况访问") {
            AlertDialog.Builder(this).setTitle("仅识别你选择的应用")
                .setMessage("授权后，EyeFlow 在本机读取前台应用的包名和切换时间，仅累计你选中的应用。不会读取屏幕内容或上传记录。你可随时在系统设置中撤销授权。")
                .setPositiveButton("前往系统设置") { _, _ -> startActivity(Intent(android.provider.Settings.ACTION_USAGE_ACCESS_SETTINGS)) }
                .setNegativeButton("取消", null).show()
        }
        button("2. 选择需要提醒的应用") { selectApps() }
        button("提醒声音与震动") {
            val prefs = getSharedPreferences("eyeflow", MODE_PRIVATE)
            val modes = arrayOf("轻震动（默认，无声音）", "静默通知", "声音和震动")
            AlertDialog.Builder(this).setTitle("到点如何提醒")
                .setSingleChoiceItems(modes, prefs.getInt("alert_mode", 0)) { dialog, which ->
                    prefs.edit().putInt("alert_mode", which).apply(); dialog.dismiss()
                }.setNegativeButton("取消", null).show()
        }
        button("中央小卡片（可选）") {
            val prefs = getSharedPreferences("eyeflow", MODE_PRIVATE)
            AlertDialog.Builder(this).setTitle("可随时收起的小卡片")
                .setMessage("默认使用通知。开启后，到点会短暂显示中央小卡片，卡片外仍可操作，10 秒后自动收起；不会全屏遮挡。需要系统悬浮窗权限。系统可限制其显示，未授权时仍用通知。")
                .setPositiveButton("开启小卡片") { _, _ ->
                    prefs.edit().putBoolean("overlay_enabled", true).apply()
                    if (!android.provider.Settings.canDrawOverlays(this)) startActivity(Intent(android.provider.Settings.ACTION_MANAGE_OVERLAY_PERMISSION, android.net.Uri.parse("package:$packageName")))
                }.setNeutralButton("仅用通知") { _, _ -> prefs.edit().putBoolean("overlay_enabled", false).apply() }
                .setNegativeButton("取消", null).show()
        }
        button("系统通知设置") { startActivity(Intent(android.provider.Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(android.provider.Settings.EXTRA_APP_PACKAGE, packageName)) }
        text("预告静默；到点默认轻震动一次、无声音。系统通知设置、静音和勿扰优先，忽略后不会连续震动或升级遮挡。小卡片不会额外发声。", 15f)
        stop = button("停止护眼会话") { startService(Intent(this, ReminderService::class.java).setAction(ReminderService.STOP)) }
        text("短休息与长休息", 20f)
        text("连续目标应用使用约 15–25 分钟（以 20 分钟为峰值）提醒短休息 30 秒；约 2 小时提醒长休息 15 分钟。目标应用之间切换继续计时。切到其他应用只暂停累计，不算休息，也不会清除待休息状态。", 15f)
        text("锁屏就不提醒", 20f)
        text("锁屏不提醒。确认连续锁屏至少 30 秒才重置短休息计时，至少 15 分钟才重置长休息计时；短暂锁屏只暂停。也可点按“现在休息”，留在 EyeFlow 主动休息；继续刷目标应用会中断本次休息，待休息状态仍保留。切到其他应用、权限丢失和未知时间都不当作休息。", 15f)
        text("Android 的边界", 20f)
        text("本应用不验证视线方向。经你授权后，仅识别所选应用是否在前台，不读取输入、聊天、屏幕内容或全屏状态。分屏、画中画和厂商限制可能影响识别。通知需要你的许可；省电、勿扰、系统回收和厂商后台限制可能延迟提醒。不会自动开机启动，不申请无障碍或精确闹钟权限。", 15f)
        text("会话和累计统计仅存于本机。进程被系统结束后，需要重新打开并点按恢复；未保存的最近一小段时间可能丢失。停止会话不会证明你已休息，待休息时间会保留，恢复后继续。旧版累计规则的会话不转换为已验证休息统计。", 14f)
        val migrationPrefs = getSharedPreferences("eyeflow", MODE_PRIVATE)
        if (migrationPrefs.contains("snapshot") && !migrationPrefs.getBoolean("legacy_migration_seen", false)) {
            text("检测到旧版累计计时记录：旧版计时与统计不符合当前规则，已从新会话中排除，不能视为已完成的有效休息。", 14f)
            migrationPrefs.edit().remove("snapshot").remove("pause_remaining").remove("completed").remove("postponed").putBoolean("legacy_migration_seen", true).apply()
        }
        setContentView(scroll)
    }
    private fun sendCommand(code: Int) {
        startService(Intent(this, ReminderService::class.java).setAction(ReminderService.COMMAND).putExtra("command", code))
    }
    private fun selectApps() {
        val prefs = getSharedPreferences("eyeflow", MODE_PRIVATE)
        val selected = (prefs.getStringSet("selected_apps", emptySet()) ?: emptySet()).toMutableSet()
        val intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        val apps = packageManager.queryIntentActivities(intent, 0)
            .filter { it.activityInfo.packageName != packageName }
            .distinctBy { it.activityInfo.packageName }.sortedBy { it.loadLabel(packageManager).toString() }
        if (apps.isEmpty()) { Toast.makeText(this, "未找到可选择的应用", Toast.LENGTH_LONG).show(); return }
        val labels = apps.map { "${it.loadLabel(packageManager)} (${it.activityInfo.packageName})" }.toTypedArray()
        val checked = apps.map { it.activityInfo.packageName in selected }.toBooleanArray()
        AlertDialog.Builder(this).setTitle("仅提醒这些应用")
            .setMultiChoiceItems(labels, checked) { _, index, enabled ->
                val name = apps[index].activityInfo.packageName
                if (enabled) selected.add(name) else selected.remove(name)
            }
            .setPositiveButton("保存") { _, _ -> prefs.edit().putStringSet("selected_apps", selected).apply() }
            .setNegativeButton("取消", null).show()
    }
    private fun requestStart() {
        if (!AppUsage.hasPermission(this)) { Toast.makeText(this, "请先自主开启使用情况访问权限", Toast.LENGTH_LONG).show(); return }
        if (getSharedPreferences("eyeflow", MODE_PRIVATE).getStringSet("selected_apps", emptySet()).isNullOrEmpty()) {
            selectApps(); return
        }
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 20)
            return
        }
        if (!getSystemService(NotificationManager::class.java).areNotificationsEnabled()) {
            AlertDialog.Builder(this).setTitle("请允许休息通知").setMessage("EyeFlow 需要通知来提醒休息。你可以在系统设置中开启，再开始会话。")
                .setPositiveButton("打开设置") { _, _ -> startActivity(Intent(android.provider.Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(android.provider.Settings.EXTRA_APP_PACKAGE, packageName)) }
                .setNegativeButton("暂不", null).show()
            return
        }
        startForegroundService(Intent(this, ReminderService::class.java).setAction(ReminderService.START))
    }
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == 20 && grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) requestStart()
        else if (requestCode == 20) Toast.makeText(this, "未开始计时。允许通知后可再试。", Toast.LENGTH_LONG).show()
    }
    private fun render() {
        val state = SessionState.snapshot
        val prefs = getSharedPreferences("eyeflow", MODE_PRIVATE)
        status.text = state.error ?: when {
            !state.running -> if (prefs.contains(ReminderService.SNAPSHOT)) "已停止 · 恢复后继续待休息状态" else "尚未开始"
            !state.deliveryAllowed -> "通知不可用 · 待休息状态仍保留"
            state.paused -> "提醒已暂停 · 目标使用仍在计时"
            state.phase == 5 || state.phase == 6 -> "主动休息中 · 继续刷目标应用会中断"
            state.state == UseState.UNKNOWN -> "状态未知 · 不推定使用或休息"
            state.state == UseState.LOCKED -> "锁屏中 · 仅连续有效锁屏算休息"
            state.state == UseState.OUT_OF_SCOPE -> "其他应用 · 本轮计时暂停"
            state.phase == 2 || state.phase == 4 -> "待休息 · 提醒不等于已完成休息"
            state.phase == 1 || state.phase == 3 -> "即将提醒休息"
            else -> "目标应用使用中"
        }
        timer.text = if (state.paused) "已暂停" else if (state.phase == 2 || state.phase == 4) "随时休息" else if (state.running) "%02d:%02d".format(state.remaining / 60, state.remaining % 60) else "—"
        stats.text = getString(R.string.session_totals, prefs.getInt("v2_completed", 0), prefs.getInt("v2_natural", 0),
            prefs.getInt("v2_postponed", 0), state.shortDebt / 60, state.longDebt / 60)
        start.visibility = if (state.running) View.GONE else View.VISIBLE
        stop.isEnabled = state.running
        val pending = state.phase in 1..4
        val resting = state.phase == 5 || state.phase == 6
        commands.forEach { (code, button) ->
            button.visibility = if (state.running && when(code) {
                1, 3 -> pending && !state.paused
                2 -> pending && !state.paused && state.canPostpone
                4 -> !state.paused && !resting
                5 -> state.paused
                7 -> resting
                else -> false
            }) View.VISIBLE else View.GONE
        }
        detail.text = when {
            !state.running -> "选择你常看的应用，再开启提醒。"
            resting -> "可以放下手机，看看远处。你随时可以结束。"
            state.paused -> "稍后再提醒；暂停不会被计为休息。"
            pending -> "忙完手上的事再决定，提醒不会接管屏幕。"
            else -> "快到时间会先预告。到点由你决定何时休息。"
        }
    }
    private fun reportVisibility() {
        SessionState.ownVisible = resumed && hasWindowFocus() && !isInMultiWindowMode && !isInPictureInPictureMode
        if (SessionState.snapshot.running) startService(Intent(this, ReminderService::class.java).setAction(ReminderService.OBSERVE))
    }
    override fun onWindowFocusChanged(hasFocus: Boolean) { super.onWindowFocusChanged(hasFocus); reportVisibility() }
    override fun onResume() { super.onResume(); resumed = true; reportVisibility(); handler.post(refresh) }
    override fun onPause() { resumed = false; reportVisibility(); handler.removeCallbacks(refresh); super.onPause() }
    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
}
