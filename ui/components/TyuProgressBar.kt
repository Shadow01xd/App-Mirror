package com.tyu.app.ui.components

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import com.tyu.app.ui.theme.*

@Composable
fun TyuProgressBar(progress: Float?) {
    if (progress == null) {
        LinearProgressIndicator(Modifier.fillMaxWidth().height(TyuDimens.Tiny).clip(TyuShapes.Pill),
            color = TyuColors.Primary, trackColor = TyuColors.Elevated)
    } else {
        val animated by animateFloatAsState(progress.coerceIn(0f, 1f),
            tween(TyuMotion.State, easing = TyuMotion.Ease), label = "Progreso")
        LinearProgressIndicator(progress = { animated },
            modifier = Modifier.fillMaxWidth().height(TyuDimens.Tiny).clip(TyuShapes.Pill),
            color = if (progress >= 1f) TyuColors.Success else TyuColors.Primary,
            trackColor = TyuColors.Elevated)
    }
}
