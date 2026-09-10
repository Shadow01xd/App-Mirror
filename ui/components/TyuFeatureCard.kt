package com.tyu.app.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import com.tyu.app.ui.icons.TyuIcons
import com.tyu.app.ui.theme.*

@Composable
fun TyuFeatureCard(title: String, subtitle: String, icon: ImageVector, onClick: () -> Unit,
    active: Boolean = false) {
    val ink by animateColorAsState(if (active) TyuColors.Primary else TyuColors.Secondary,
        tween(TyuMotion.State), label = "Estado del servicio")
    Surface(onClick = onClick, modifier = Modifier.fillMaxWidth(), color = TyuColors.Background,
        shape = TyuShapes.Control) {
        Column {
            Row(Modifier.padding(vertical = TyuDimens.Gap).heightIn(min = TyuDimens.Touch),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(TyuDimens.Gap)) {
                Icon(icon, null, tint = ink, modifier = Modifier.size(TyuDimens.Icon))
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(TyuDimens.Tiny)) {
                    Text(title, style = MaterialTheme.typography.titleMedium)
                    if (active) TyuStatusDot(subtitle) else Text(subtitle, color = TyuColors.Secondary,
                        style = MaterialTheme.typography.bodySmall)
                }
                Icon(TyuIcons.Chevron, null, tint = TyuColors.Muted)
            }
            HorizontalDivider(color = TyuColors.Border)
        }
    }
}
