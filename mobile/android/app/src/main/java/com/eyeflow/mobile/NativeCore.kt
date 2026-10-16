package com.eyeflow.mobile

internal object NativeCore {
    init { System.loadLibrary("eyeflow_jni") }
    @JvmStatic external fun exportState(handle: Long): ByteArray?
    @JvmStatic external fun importState(handle: Long, data: ByteArray): Boolean
    @JvmStatic external fun create(): Long
    @JvmStatic external fun destroy(handle: Long)
    @JvmStatic external fun command(handle: Long, command: Int): Int
    @JvmStatic external fun advance(handle: Long, durationMs: Long, state: Int, deliveryAllowed: Boolean): Int
    @JvmStatic external fun remaining(handle: Long): Long
    @JvmStatic external fun completed(handle: Long): Int
    @JvmStatic external fun postponed(handle: Long): Int
    @JvmStatic external fun natural(handle: Long): Int
    @JvmStatic external fun paused(handle: Long): Long
    @JvmStatic external fun shortDebt(handle: Long): Long
    @JvmStatic external fun longDebt(handle: Long): Long
    @JvmStatic external fun reminderId(handle: Long): Long
    @JvmStatic external fun canPostpone(handle: Long): Boolean
    @JvmStatic external fun canDeliver(handle: Long): Boolean
}
internal data class SessionSnapshot(val running: Boolean = false, val phase: Int = 0,
    val remaining: Long = 0, val paused: Boolean = false, val error: String? = null,
    val state: UseState = UseState.STOPPED, val deliveryAllowed: Boolean = false,
    val shortDebt: Long = 0, val longDebt: Long = 0, val canPostpone: Boolean = false)
internal object SessionState {
    @Volatile var snapshot = SessionSnapshot()
    @Volatile var ownVisible = false
}
