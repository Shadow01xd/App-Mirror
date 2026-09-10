package com.tyu.app.ui.theme
import androidx.compose.animation.core.CubicBezierEasing

/** Compose respects Android's animator duration scale, including Remove animations. */
object TyuMotion {
    const val Press = 140
    const val State = 220
    const val Navigate = 280
    const val Link = 650
    val Ease = CubicBezierEasing(0.2f, 0f, 0f, 1f)
}
