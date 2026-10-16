package com.eyeflow.mobile

import android.content.Context
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.drawable.GradientDrawable
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

/** A real small window, never a transparent full-screen touch interceptor. */
internal class ReminderCard(private val context: Context) {
    private val manager = context.getSystemService(WindowManager::class.java)
    private val handler = Handler(Looper.getMainLooper())
    private var view: View? = null
    private val dismiss = Runnable { hide() }
    private fun trace(event: String) {
        if ((context.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0)
            android.util.Log.d("EyeFlowCardQA", "$event elapsed=${android.os.SystemClock.elapsedRealtime()}")
    }
    fun hide() {
        handler.removeCallbacks(dismiss)
        view?.let { trace("hide"); try { manager.removeView(it) } catch (_: IllegalArgumentException) {} }
        view = null
    }
    fun show(longRest: Boolean, canPostpone: Boolean, action: (Int) -> Unit) {
        hide()
        val density = context.resources.displayMetrics.density
        fun dp(v: Int) = (v * density).toInt()
        val metrics = context.resources.displayMetrics
        // Large font / short landscape surfaces fall back to notification only.
        if (metrics.heightPixels / density < 500 || context.resources.configuration.fontScale > 1.3f) return
        val card = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL; setPadding(dp(16), dp(12), dp(16), dp(8))
            background = GradientDrawable().apply { setColor(Color.rgb(243,247,244)); cornerRadius=dp(20).toFloat(); setStroke(dp(1),Color.rgb(180,201,188)) }
            elevation = dp(8).toFloat()
        }
        card.addView(TextView(context).apply { text = if(longRest) "找个方便的时间，休息一会儿" else "眼睛想歇一会儿了"; textSize=18f; setTextColor(Color.rgb(23,60,48)) })
        card.addView(TextView(context).apply { text="忙完这一步再决定 · 10 秒后收起"; textSize=13f; setPadding(0,dp(6),0,dp(6)); setTextColor(Color.DKGRAY) })
        val row = LinearLayout(context)
        listOf("稍后5分" to 2, "略过" to 3, "收起" to 0).filter { it.second != 2 || canPostpone }.forEach { (label, code) ->
            row.addView(Button(context).apply { text=label; textSize=12f; minWidth=0; minimumWidth=0; setPadding(0,0,0,0); setOnClickListener { hide(); if(code!=0) action(code) } }, LinearLayout.LayoutParams(0,dp(48),1f))
        }
        card.addView(row)
        val params=WindowManager.LayoutParams(minOf(dp(320),(metrics.widthPixels*.88).toInt()), WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
            PixelFormat.TRANSLUCENT).apply { gravity=Gravity.CENTER }
        try { manager.addView(card, params); view=card; trace("show"); handler.postDelayed(dismiss,10_000) }
        catch (_: SecurityException) { view=null }
        catch (_: WindowManager.BadTokenException) { view=null }
    }
}
