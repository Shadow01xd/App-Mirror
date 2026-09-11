package com.tyu.app.ui.components

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import com.tyu.app.ui.theme.*

@Composable
fun TyuToggle(title: String, subtitle: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit,
    icon: ImageVector? = null, enabled: Boolean = true) {
    val fontScale = LocalDensity.current.fontScale
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val stacked = maxWidth < 260.dp && fontScale > 1.1f
        val control: @Composable () -> Unit = {
            Switch(checked, onCheckedChange = null, enabled = enabled,
                colors = SwitchDefaults.colors(
                    checkedThumbColor = TyuColors.OnPrimary, checkedTrackColor = TyuColors.Primary,
                    checkedBorderColor = TyuColors.Primary, uncheckedThumbColor = TyuColors.Muted,
                    uncheckedTrackColor = TyuColors.Surface, uncheckedBorderColor = TyuColors.Border))
        }
        val touch = Modifier.fillMaxWidth().heightIn(min = TyuDimens.Touch)
            .toggleable(checked, enabled = enabled, role = Role.Switch, onValueChange = onCheckedChange)
            .padding(vertical = TyuDimens.Small)
        if (stacked) {
            Column(touch, verticalArrangement = Arrangement.spacedBy(TyuDimens.Small)) {
                Text(title, style = MaterialTheme.typography.bodyLarge,
                    color = if (enabled) TyuColors.Text else TyuColors.Muted)
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(TyuDimens.Medium)) {
                    Text(subtitle, Modifier.weight(1f), color = TyuColors.Secondary, style = MaterialTheme.typography.bodySmall)
                    control()
                }
            }
        } else {
            Row(touch, verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(TyuDimens.Medium)) {
                if (icon != null) Icon(icon, null, tint = TyuColors.Secondary, modifier = Modifier.size(TyuDimens.Icon))
                Column(Modifier.weight(1f)) {
                    Text(title, style = MaterialTheme.typography.bodyLarge,
                        color = if (enabled) TyuColors.Text else TyuColors.Muted)
                    if (subtitle.isNotEmpty()) Text(subtitle, color = TyuColors.Secondary, style = MaterialTheme.typography.bodySmall)
                }
                control()
            }
        }
    }
}
