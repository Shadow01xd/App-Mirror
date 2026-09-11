package com.tyu.app.ui.components

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import com.tyu.app.ui.icons.*
import com.tyu.app.ui.theme.*

@Composable
fun TyuTopBar(onBack: (() -> Unit)? = null, onSettings: (() -> Unit)? = null,
    onSessions: (() -> Unit)? = null) {
    Column {
        Row(Modifier.fillMaxWidth().padding(horizontal = TyuDimens.Gap, vertical = TyuDimens.Small),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(TyuDimens.Small)) {
            if (onBack != null) TyuIconButton(TyuIcons.Back, "Volver", onBack)
            Text("TYU", color = TyuColors.Primary, style = MaterialTheme.typography.displaySmall,
                modifier = Modifier.weight(1f).padding(start = if (onBack == null) TyuDimens.Small else TyuDimens.Tiny))
            if (onSessions != null) TyuIconButton(TyuIcons.Activity, "Actividad actual", onSessions)
            if (onSettings != null) TyuIconButton(SettingsIcon, "Configuración", onSettings)
        }
        HorizontalDivider(Modifier.padding(horizontal = TyuDimens.Page), color = TyuColors.Surface)
    }
}
