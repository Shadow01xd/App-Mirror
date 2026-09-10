package com.tyu.app.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import com.tyu.app.model.UiMode
import com.tyu.app.ui.icons.icon
import com.tyu.app.ui.theme.*

/** Mutually exclusive connection modes, retaining the original navigation behavior. */
@Composable
fun TyuModeSelector(selected: UiMode, onSelect: (UiMode) -> Unit) {
    Row(Modifier.fillMaxWidth().padding(vertical = TyuDimens.Medium).clip(TyuShapes.Card)
        .background(TyuColors.Surface).padding(TyuDimens.Small).selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(TyuDimens.Small)) {
        UiMode.entries.forEach { mode ->
            val active = mode == selected
            val background by animateColorAsState(if (active) TyuColors.Primary else TyuColors.Surface,
                tween(TyuMotion.State), label = "Selección de modo")
            val foreground by animateColorAsState(if (active) TyuColors.OnPrimary else TyuColors.Secondary,
                tween(TyuMotion.State), label = "Icono de modo")
            Column(Modifier.weight(1f).clip(TyuShapes.Control).background(background)
                .selectable(active, role = Role.Tab, onClick = { onSelect(mode) })
                .heightIn(min = TyuDimens.ButtonHeight).padding(vertical = TyuDimens.Medium),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(TyuDimens.Tiny)) {
                Icon(mode.icon(), null, tint = foreground, modifier = Modifier.size(TyuDimens.Icon))
                Text(mode.title, style = MaterialTheme.typography.labelSmall, color = foreground)
            }
        }
    }
}
