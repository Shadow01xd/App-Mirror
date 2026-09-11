package com.tyu.app.ui.theme

import android.provider.Settings
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.interaction.InteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

/**
 * One motion vocabulary for the whole app. Compose already honours Android's animator
 * duration scale (including "Remove animations"), so timing shrinks to zero for users who
 * asked for that; [rememberReduceMotion] lets us drop looping and travelling motion entirely.
 */
object TyuMotion {
    /** Button / tap acknowledgement. */
    const val Press = 140
    /** A control changing state in place (selection, colour, progress). */
    const val State = 220
    /** A whole screen entering or leaving. */
    const val Navigate = 280
    /** A schematic drawing revealing itself once. */
    const val Link = 650

    /** Strong ease-out — anything entering, and anything the eye is tracking on arrival. */
    val Ease = CubicBezierEasing(0.2f, 0f, 0f, 1f)
    /** Symmetric ease-in-out — for elements that travel or morph across the screen. */
    val EaseInOut = CubicBezierEasing(0.65f, 0f, 0.35f, 1f)

    /** Press feedback: quick, no overshoot. */
    fun <T> pressSpring(): SpringSpec<T> =
        spring(stiffness = Spring.StiffnessMedium)

    /** A value settling into a new position on screen — a hair of give, never a bounce. */
    fun <T> spatialSpring(): SpringSpec<T> =
        spring(dampingRatio = 0.82f, stiffness = Spring.StiffnessMediumLow)
}

/** True when the user has turned system animations off. Use it to skip loops and travel. */
@Composable
fun rememberReduceMotion(): Boolean {
    val resolver = LocalContext.current.contentResolver
    return remember(resolver) {
        Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    }
}

/**
 * Every pressable surface should cede a little under the finger. Feed this the same
 * [InteractionSource] the click/selectable modifier uses.
 */
fun Modifier.tyuPressScale(
    interactionSource: InteractionSource,
    enabled: Boolean = true,
    pressedScale: Float = 0.97f,
): Modifier = composed {
    val pressed by interactionSource.collectIsPressedAsState()
    val reduce = rememberReduceMotion()
    val scale by androidx.compose.animation.core.animateFloatAsState(
        targetValue = if (pressed && enabled && !reduce) pressedScale else 1f,
        animationSpec = TyuMotion.pressSpring(),
        label = "tyuPressScale",
    )
    graphicsLayer { scaleX = scale; scaleY = scale }
}

/**
 * A one-shot entrance: fade up a few pixels. Pass the item's position to stagger a list;
 * collapses to nothing when the user has reduced motion.
 */
fun Modifier.tyuEnter(index: Int = 0): Modifier = composed {
    if (rememberReduceMotion()) return@composed this
    val progress = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        if (index > 0) delay(index * 40L)
        progress.animateTo(1f, tween(TyuMotion.State, easing = TyuMotion.Ease))
    }
    graphicsLayer {
        alpha = progress.value
        translationY = (1f - progress.value) * 12.dp.toPx()
    }
}
