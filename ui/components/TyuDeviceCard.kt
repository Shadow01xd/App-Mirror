package com.tyu.app.ui.components

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import com.tyu.app.model.UiDevice
import com.tyu.app.ui.icons.TyuIcons
import com.tyu.app.ui.theme.*

@Composable
fun TyuDeviceCard(device: UiDevice, onConnect: () -> Unit) {
    Surface(color = TyuColors.Surface, shape = TyuShapes.Card, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(TyuDimens.Gap), verticalArrangement = Arrangement.spacedBy(TyuDimens.Medium)) {
            Row(verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(TyuDimens.Gap)) {
                Icon(TyuIcons.Desktop, null, tint = TyuColors.Secondary, modifier = Modifier.size(TyuDimens.Large))
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(TyuDimens.Tiny)) {
                    Text(device.computerName, style = MaterialTheme.typography.titleLarge)
                    Text(device.status.label, color = TyuColors.Secondary, style = MaterialTheme.typography.bodySmall)
                }
            }
            TyuSecondaryButton("Conectar", onConnect, Modifier.fillMaxWidth(), TyuIcons.Arrow)
        }
    }
}
