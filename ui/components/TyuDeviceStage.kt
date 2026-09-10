package com.tyu.app.ui.components

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
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
import kotlin.math.pow

/**
 * Schematic of the two endpoints, not a video preview or an operational connection.
 * [toPhone] points the travelling signal: true = PC → teléfono (monitor), false = teléfono → PC (espejo).
 */
@Composable
fun TyuDeviceStage(modifier: Modifier = Modifier, linked: Boolean = true, toPhone: Boolean = true) {
    val trace = remember { Animatable(0f) }
    // The signal travelling the wire accelerates away and eases into the far endpoint.
    LaunchedEffect(linked, toPhone) { trace.snapTo(0f); trace.animateTo(1f, tween(TyuMotion.Link, easing = TyuMotion.EaseInOut)) }
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
        val deskAnchor = Offset(x + dw * 0.34f, y + dh * 0.56f)
        val phoneAnchor = Offset(px + pw / 2, deskAnchor.y)
        val origin = if (toPhone) deskAnchor else phoneAnchor
        val target = if (toPhone) phoneAnchor else deskAnchor
        drawLine(TyuColors.Border, deskAnchor, phoneAnchor, line, StrokeCap.Round)
        drawLine(accent, origin, Offset(origin.x + (target.x - origin.x) * trace.value, origin.y), line * 1.5f, StrokeCap.Round)
        drawCircle(accent, line * 2.5f, origin)
        drawCircle(accent, line * 2.5f, target, style = Stroke(line))
        drawLine(TyuColors.Border, Offset(x, h * 0.94f), Offset(px + pw, h * 0.94f), 1.dp.toPx())
    }
}

@Composable
fun TyuSignalStage(icon: ImageVector, modifier: Modifier = Modifier, error: Boolean = false) {
    val accent = if (error) TyuColors.Error else TyuColors.Primary
    val animate = !error && !rememberReduceMotion()
    // One phenomenon: the equipment pings its presence, the wavefront crosses the frame,
    // and the nearby transmitter blips back the instant the front reaches it.
    val clock = if (animate) {
        rememberInfiniteTransition(label = "búsqueda de equipo").animateFloat(
            initialValue = 0f, targetValue = 1f,
            animationSpec = infiniteRepeatable(tween(2600, easing = LinearEasing), RepeatMode.Restart),
            label = "sonar",
        ).value
    } else 0f

    Box(modifier.clip(TyuShapes.Card).background(TyuColors.Stage), contentAlignment = Alignment.Center) {
        Canvas(Modifier.fillMaxSize()) {
            val center = Offset(size.width / 2f, size.height / 2f)
            val radius = size.height * 0.33f
            val ringRight = center.x + radius
            val endRight = size.width * 0.92f
            val hair = 1.dp.toPx()
            val reach = endRight - center.x

            drawLine(TyuColors.Border, Offset(size.width * 0.08f, center.y), Offset(center.x - radius, center.y), hair)
            drawLine(TyuColors.Border, Offset(ringRight, center.y), Offset(endRight, center.y), hair)

            // expanding wavefronts, half a cycle apart
            if (animate) {
                repeat(2) { i ->
                    val p = (clock + i * 0.5f) % 1f
                    val eased = 1f - (1f - p).pow(2.4f)
                    val r = radius + eased * (reach - radius)
                    val alpha = (1f - p).pow(1.7f) * 0.42f
                    drawCircle(accent.copy(alpha = alpha), r, center, style = Stroke(hair * (1.7f - p)))
                }
            }

            // the equipment ring (calm anchor)
            drawCircle(TyuColors.Border, radius, center, style = Stroke(hair))

            // the nearby transmitter, answering as each front arrives (twice per cycle)
            val answer = if (animate) (1f - (clock * 2f) % 1f).pow(4f) else 0f
            if (answer > 0f) {
                drawCircle(accent.copy(alpha = answer * 0.22f), 9.dp.toPx() + answer * 4.dp.toPx(), Offset(endRight, center.y))
            }
            drawCircle(accent, 3.dp.toPx() + answer * 1.5.dp.toPx(), Offset(endRight, center.y))
        }
        Icon(icon, null, Modifier.size(48.dp), tint = accent)
    }
}
