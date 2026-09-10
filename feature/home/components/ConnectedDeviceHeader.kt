package com.tyu.app.feature.home.components

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.dp
import com.tyu.app.model.UiDevice
import com.tyu.app.ui.components.*
import com.tyu.app.ui.icons.TyuIcons
import com.tyu.app.ui.theme.*

@Composable
fun ConnectedDeviceHeader(device: UiDevice, onDetails: () -> Unit, toPhone: Boolean = true) {
    val compact = LocalConfiguration.current.screenHeightDp < 500
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TyuDimens.Gap)) {
        TyuHeading(device.name)
        TyuStatusDot("Conectado")
        if (!compact) TyuDeviceStage(
            Modifier.fillMaxWidth().height(TyuDimens.HeroIcon * 1.5f),
            toPhone = toPhone,
        )
        Surface(onClick = onDetails, color = TyuColors.Background, shape = TyuShapes.Control) {
            Row(Modifier.fillMaxWidth().heightIn(min = TyuDimens.ButtonHeight).padding(vertical = TyuDimens.Small),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(TyuDimens.Medium)) {
                Icon(TyuIcons.Desktop, null, tint = TyuColors.Secondary)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(TyuDimens.Tiny)) {
                    Text(device.computerName, style = MaterialTheme.typography.titleMedium)
                    Text("${device.connectionQuality} conexión", color = TyuColors.Secondary, style = MaterialTheme.typography.bodySmall)
                }
                Icon(TyuIcons.Chevron, null, tint = TyuColors.Secondary)
            }
        }
        HorizontalDivider(color = TyuColors.Border)
    }
}
