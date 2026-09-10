package com.tyu.app.navigation

import androidx.compose.animation.*
import androidx.compose.animation.core.tween
import com.tyu.app.ui.theme.TyuMotion

/**
 * Screens slide the way the journey moves: forward pushes the new screen in from the trailing
 * edge, Back lets it fall back toward where it came from. The outgoing screen drifts a shorter
 * distance so the two never look glued together.
 */
fun tyuNavigationTransition(forward: Boolean): ContentTransform {
    val direction = if (forward) 1 else -1
    return (fadeIn(tween(TyuMotion.Navigate, easing = TyuMotion.Ease)) +
        slideInHorizontally(tween(TyuMotion.Navigate, easing = TyuMotion.Ease)) { (it / 10) * direction }) togetherWith
        (fadeOut(tween(TyuMotion.Press)) +
            slideOutHorizontally(tween(TyuMotion.Navigate, easing = TyuMotion.Ease)) { -(it / 28) * direction })
}
