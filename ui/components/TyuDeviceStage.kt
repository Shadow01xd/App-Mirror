package com.tyu.app.ui.components

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Icon
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import com.tyu.app.ui.theme.*

/** Schematic of the two endpoints, not a video preview or an operational connection. */
@Composable
fun TyuDeviceStage(modifier: Modifier = Modifier, linked: Boolean = true) {
    val trace = remember { Animatable(0f) }
    LaunchedEffect(linked) { trace.snapTo(0f); trace.animateTo(1f, tween(TyuMotion.Link, easing = TyuMotion.Ease)) }
    Canvas(modifier.clip(TyuShapes.Card).background(TyuColors.Stage)) {
        val w = size.width
        val h = size.height
        val line = 1.5.dp.toPx()
        val accent = if (linked) TyuColors.Primary else TyuColors.Secondary
        val x = w * 0.10f; val y = h * 0.16f
        val dw = w * 0.52f; val dh = h * 0.54f
        drawRoundRect(TyuColors.Elevated, Offset(x, y), Size(dw, dh), CornerRadius(8.dp.toPx()))
        drawRoundRect(TyuColors.Border, Offset(x, y), Size(dw, dh), CornerRadius(8.dp.toPx()), style = Stroke(line))
        drawRoundRect(TyuColors.Background, Offset(x + line * 3, y + line * 3), Size(dw - line * 6, dh - line * 6), CornerRadius(4.dp.toPx()))
        drawLine(TyuColors.Secondary, Offset(x + dw / 2, y + dh), Offset(x + dw / 2, h * 0.82f), line, StrokeCap.Round)
        drawLine(TyuColors.Secondary, Offset(x + dw * 0.34f, h * 0.82f), Offset(x + dw * 0.66f, h * 0.82f), line, StrokeCap.Round)
        val px = w * 0.72f; val py = h * 0.30f; val pw = w * 0.16f; val ph = h * 0.55f
        drawRoundRect(TyuColors.Surface, Offset(px, py), Size(pw, ph), CornerRadius(9.dp.toPx()))
        drawRoundRect(TyuColors.Secondary, Offset(px, py), Size(pw, ph), CornerRadius(9.dp.toPx()), style = Stroke(line))
        drawLine(TyuColors.Muted, Offset(px + pw * 0.35f, py + line * 4), Offset(px + pw * 0.65f, py + line * 4), line, StrokeCap.Round)
        drawLine(accent, Offset(px + pw * 0.3f, py + ph - line * 4), Offset(px + pw * 0.7f, py + ph - line * 4), line, StrokeCap.Round)
        val from = Offset(x + dw * 0.34f, y + dh * 0.56f)
        val to = Offset(px + pw / 2, from.y)
        drawLine(TyuColors.Border, from, to, line, StrokeCap.Round)
        drawLine(accent, from, Offset(from.x + (to.x - from.x) * trace.value, from.y), line * 1.5f, StrokeCap.Round)
        drawCircle(accent, line * 2.5f, from)
        drawCircle(accent, line * 2.5f, to, style = Stroke(line))
        drawLine(TyuColors.Border, Offset(x, h * 0.94f), Offset(px + pw, h * 0.94f), 1.dp.toPx())
    }
}

@Composable
fun TyuSignalStage(icon: ImageVector, modifier: Modifier = Modifier, error: Boolean = false) {
    val accent = if (error) TyuColors.Error else TyuColors.Primary
    Box(modifier.clip(TyuShapes.Card).background(TyuColors.Stage), contentAlignment = Alignment.Center) {
        Canvas(Modifier.fillMaxSize()) {
            val center = Offset(size.width / 2, size.height / 2)
            val radius = size.height * 0.33f
            drawCircle(TyuColors.Border, radius, center, style = Stroke(1.dp.toPx()))
            drawLine(TyuColors.Border, Offset(size.width * 0.08f, center.y), Offset(center.x - radius, center.y), 1.dp.toPx())
            drawLine(TyuColors.Border, Offset(center.x + radius, center.y), Offset(size.width * 0.92f, center.y), 1.dp.toPx())
            drawCircle(accent, 3.dp.toPx(), Offset(size.width * 0.92f, center.y))
        }
        Icon(icon, null, Modifier.size(48.dp), tint = accent)
    }
}
