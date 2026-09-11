package com.tyu.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.tyu.app.model.UiDevice
import com.tyu.app.model.UiDeviceStatus
import com.tyu.app.ui.icons.TyuIcons
import com.tyu.app.ui.theme.*

/**
 * One entry in the nearby-devices picker. The whole row is the connect target; the closest
 * equipment reads at full signal, in accent, so the eye lands on it first.
 */
@Composable
fun TyuDeviceRow(device: UiDevice, onConnect: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val near = device.status == UiDeviceStatus.Nearby
    val accent = if (near) TyuColors.Primary else TyuColors.Secondary

    Surface(onClick = onConnect, color = Color.Transparent, interactionSource = interaction,
        modifier = Modifier.fillMaxWidth().tyuPressScale(interaction)) {
        BoxWithConstraints(Modifier.fillMaxWidth().heightIn(min = 72.dp)
            .padding(horizontal = TyuDimens.Gap, vertical = TyuDimens.Medium)) {
            val stacked = maxWidth < 320.dp

            val signal = @Composable {
                Box(Modifier.size(40.dp).clip(TyuShapes.Control).background(TyuColors.Background)
                    .then(if (near) Modifier.border(1.dp, TyuColors.Primary, TyuShapes.Control) else Modifier),
                    contentAlignment = Alignment.Center) {
                    TyuSignalStrength(active = if (near) 3 else 2, tint = accent)
                }
            }
            val identity = @Composable { mod: Modifier ->
                Column(mod, verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(device.computerName, style = MaterialTheme.typography.titleMedium,
                        color = TyuColors.Text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(device.status.label, style = MaterialTheme.typography.labelMedium,
                        color = if (near) TyuColors.Primary else TyuColors.Secondary,
                        maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            val action = @Composable {
                Row(verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(TyuDimens.Tiny)) {
                    Text("Conectar", style = MaterialTheme.typography.labelLarge, color = TyuColors.Primary)
                    Icon(TyuIcons.Arrow, null, Modifier.size(18.dp), tint = TyuColors.Primary)
                }
            }

            if (stacked) {
                Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TyuDimens.Medium)) {
                    Row(verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(TyuDimens.Gap)) {
                        signal(); identity(Modifier.weight(1f))
                    }
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) { action() }
                }
            } else {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(TyuDimens.Gap)) {
                    signal(); identity(Modifier.weight(1f)); action()
                }
            }
        }
    }
}

/** Three ascending bars; [active] of them carry [tint], the rest sit as a hairline ghost. */
@Composable
private fun TyuSignalStrength(active: Int, tint: Color) {
    Canvas(Modifier.size(TyuDimens.Icon)) {
        val bars = 3
        val gap = size.width * 0.16f
        val barW = (size.width - gap * (bars - 1)) / bars
        for (i in 0 until bars) {
            val h = size.height * (0.4f + i * 0.3f)
            drawRoundRect(
                color = if (i < active) tint else TyuColors.Border,
                topLeft = Offset(i * (barW + gap), size.height - h),
                size = Size(barW, h),
                cornerRadius = CornerRadius(barW * 0.3f),
            )
        }
    }
}
