package com.tyu.app.ui.components

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import com.tyu.app.ui.theme.*

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TyuConnectionBanner(computerName: String, connected: Boolean = true) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TyuDimens.Medium)) {
        FlowRow(Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(TyuDimens.Gap),
            verticalArrangement = Arrangement.spacedBy(TyuDimens.Small)) {
            Text(computerName, style = MaterialTheme.typography.bodyMedium, color = TyuColors.Text)
            TyuStatusDot(if (connected) "Conectado" else "Desconectado", connected)
        }
        HorizontalDivider(color = TyuColors.Surface)
    }
}
