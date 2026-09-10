package com.tyu.app.ui.components

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import com.tyu.app.ui.icons.*
import com.tyu.app.ui.theme.*
import kotlin.math.abs
import kotlin.math.sin

@Composable
fun TyuDirectionStrip(from: String, to: String, bidirectional: Boolean = false) {
    Surface(color = TyuColors.Surface, shape = TyuShapes.Control, modifier = Modifier.fillMaxWidth()) {
        Row(Modifier.padding(horizontal = TyuDimens.Gap, vertical = TyuDimens.Medium),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TyuDimens.Medium)) {
            Text(from, style = MaterialTheme.typography.bodyMedium)
            HorizontalDivider(Modifier.weight(1f), color = TyuColors.Border)
            Icon(if (bidirectional) com.tyu.app.model.UiMode.Bypass.icon() else TyuIcons.Arrow,
                if (bidirectional) "En ambas direcciones" else "Hacia", tint = TyuColors.Primary)
            HorizontalDivider(Modifier.weight(1f), color = TyuColors.Border)
            Text(to, style = MaterialTheme.typography.bodyMedium)
        }
    }
}

@Composable
fun TyuCameraPreview() {
    Box(Modifier.fillMaxWidth().aspectRatio(16f / 9f).clip(TyuShapes.Card).background(TyuColors.Stage),
        contentAlignment = Alignment.Center) {
        Canvas(Modifier.fillMaxSize()) {
            val inset = 18.dp.toPx(); val length = 18.dp.toPx(); val stroke = 1.5.dp.toPx()
            for (x in listOf(inset, size.width - inset)) {
                for (y in listOf(inset, size.height - inset)) {
                    val dx = if (x < size.width / 2) length else -length
                    val dy = if (y < size.height / 2) length else -length
                    drawLine(TyuColors.Primary, Offset(x, y), Offset(x + dx, y), stroke)
                    drawLine(TyuColors.Primary, Offset(x, y), Offset(x, y + dy), stroke)
                }
            }
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(TyuDimens.Small)) {
            Icon(CameraIcon, null, Modifier.size(36.dp), tint = TyuColors.Secondary)
            Text("Vista previa de cámara", color = TyuColors.Secondary, style = MaterialTheme.typography.bodySmall)
        }
    }
}

/** One finite reveal of an illustrative waveform; never reads audio or implies measured levels. */
@Composable
fun TyuAudioStage(active: Boolean = false) {
    val reveal = remember { Animatable(0f) }
    LaunchedEffect(active) { reveal.snapTo(0f); reveal.animateTo(1f, tween(TyuMotion.Link, easing = TyuMotion.Ease)) }
    Canvas(Modifier.fillMaxWidth().height(112.dp).clip(TyuShapes.Card).background(TyuColors.Stage)
        .clearAndSetSemantics { }) {
        val count = 37
        val spacing = size.width / (count + 5)
        val mid = size.height / 2
        repeat(count) { index ->
            val envelope = 1f - abs(index - 18f) / 22f
            val amplitude = (8.dp.toPx() + abs(sin(index * 1.8f)) * mid * 0.65f * envelope) * (0.25f + reveal.value * 0.75f)
            val x = spacing * (index + 3)
            drawLine(if (active) TyuColors.Primary else TyuColors.Secondary, Offset(x, mid - amplitude / 2),
                Offset(x, mid + amplitude / 2), 3.dp.toPx(), StrokeCap.Round)
        }
    }
}
