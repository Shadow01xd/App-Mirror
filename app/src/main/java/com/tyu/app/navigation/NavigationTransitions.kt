package com.tyu.app.navigation

import androidx.compose.animation.*
import androidx.compose.animation.core.tween
import com.tyu.app.ui.theme.TyuMotion

fun tyuNavigationTransition(): ContentTransform =
    (fadeIn(tween(TyuMotion.Navigate, easing = TyuMotion.Ease)) +
        slideInHorizontally(tween(TyuMotion.Navigate, easing = TyuMotion.Ease)) { it / 18 }) togetherWith
        fadeOut(tween(TyuMotion.Press))
