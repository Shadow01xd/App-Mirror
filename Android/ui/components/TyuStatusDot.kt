package com.tyu.app.ui.components

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.*
import com.tyu.app.ui.theme.*

@Composable
fun TyuStatusDot(text: String, active: Boolean = true, error: Boolean = false) {
    val color = if (error) TyuColors.Error else if (active) TyuColors.Success else TyuColors.Muted
    // A live link earns a heartbeat: the dot echoes itself outward and fades, once every ~1.8s.
    val live = active && !error && !rememberReduceMotion()
    Row(Modifier.semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(TyuDimens.Small)) {
        Box(Modifier.size(TyuDimens.Small), contentAlignment = Alignment.Center) {
            if (live) {
                val pulse = rememberInfiniteTransition(label = "estado en vivo")
                val t by pulse.animateFloat(
                    initialValue = 0f, targetValue = 1f,
                    animationSpec = infiniteRepeatable(
                        tween(1800, easing = TyuMotion.Ease), RepeatMode.Restart),
                    label = "eco",
                )
                Box(Modifier.size(TyuDimens.Small).graphicsLayer {
                    val s = 1f + t * 1.4f
                    scaleX = s; scaleY = s; alpha = (1f - t) * 0.45f
                }.background(color, CircleShape))
            }
            Box(Modifier.size(TyuDimens.Small).background(color, CircleShape))
        }
        Text(text, color = if (error) TyuColors.Error else if (active) TyuColors.Success else TyuColors.Secondary,
            style = MaterialTheme.typography.bodyMedium)
    }
}
