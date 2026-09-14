package com.tyu.app.backend

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.GestureDescription
import android.content.res.Resources
import android.graphics.Path
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo

/**
 * Replays touches sent from the PC over the Mirror session. Android only lets a non-root app
 * inject input through an accessibility service, so the user enables "TYU control táctil" once.
 * Coordinates arrive normalized to the mirrored picture and map onto the full display.
 */
class TouchInjectionService : AccessibilityService() {
    private val main = Handler(Looper.getMainLooper())
    private var path: Path? = null
    private var startedAt = 0L
    private var moved = false

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {}
    override fun onInterrupt() {}
    override fun onServiceConnected() { instance = this }
    override fun onDestroy() {
        if (instance === this) instance = null
        super.onDestroy()
    }

    fun inject(kind: String, nx: Float, ny: Float) {
        val metrics = Resources.getSystem().displayMetrics
        val x = (nx * metrics.widthPixels).coerceIn(0f, metrics.widthPixels - 1f)
        val y = (ny * metrics.heightPixels).coerceIn(0f, metrics.heightPixels - 1f)
        main.post {
            when (kind) {
                "down" -> { path = Path().apply { moveTo(x, y) }; startedAt = SystemClock.uptimeMillis(); moved = false }
                "move" -> path?.let { it.lineTo(x, y); moved = true }
                "up" -> {
                    val p = path ?: return@post
                    path = null
                    if (moved) p.lineTo(x, y)
                    // ponytail: a drag is replayed on release as one stroke; live tracking needs continueStroke.
                    val duration = (SystemClock.uptimeMillis() - startedAt).coerceIn(40L, 2_000L)
                    val gesture = GestureDescription.Builder()
                        .addStroke(GestureDescription.StrokeDescription(p, 0, duration))
                        .build()
                    dispatchGesture(gesture, null, null)
                }
            }
        }
    }

    /** Types into the focused text field by rewriting its text (the only non-root way). */
    fun type(text: String) {
        main.post {
            val node = rootInActiveWindow?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: return@post
            setText(node, (node.text?.toString() ?: "") + text)
        }
    }

    /** Android key codes sent by the PC: DEL (backspace), ENTER, BACK, FORWARD_DEL. */
    fun key(code: Int) {
        main.post {
            when (code) {
                4 -> performGlobalAction(GLOBAL_ACTION_BACK)
                3 -> performGlobalAction(GLOBAL_ACTION_HOME)
                67, 112 -> {
                    val node = rootInActiveWindow?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: return@post
                    setText(node, (node.text?.toString() ?: "").dropLast(1))
                }
                66 -> {
                    val node = rootInActiveWindow?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: return@post
                    if (Build.VERSION.SDK_INT >= 30) node.performAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id)
                    else setText(node, (node.text?.toString() ?: "") + "\n")
                }
            }
        }
    }

    private fun setText(node: AccessibilityNodeInfo, value: String) {
        val args = Bundle().apply { putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, value) }
        node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)
    }

    companion object {
        @Volatile var instance: TouchInjectionService? = null
    }
}
