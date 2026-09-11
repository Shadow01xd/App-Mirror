package com.tyu.app.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.semantics.Role
import com.tyu.app.ui.theme.*

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TyuOptions(title: String, choices: List<String>, selected: String, onSelect: (String) -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TyuDimens.Medium)) {
        TyuSectionTitle(title)
        FlowRow(Modifier.fillMaxWidth().selectableGroup(),
            horizontalArrangement = Arrangement.spacedBy(TyuDimens.Small),
            verticalArrangement = Arrangement.spacedBy(TyuDimens.Small)) {
            choices.forEach { choice ->
                val active = selected == choice
                val fill by animateColorAsState(if (active) TyuColors.PrimarySurface else TyuColors.Surface,
                    tween(TyuMotion.State, easing = TyuMotion.Ease), label = "Fondo de opción")
                val stroke by animateColorAsState(if (active) TyuColors.Primary else TyuColors.Border,
                    tween(TyuMotion.State, easing = TyuMotion.Ease), label = "Borde de opción")
                val interaction = remember { MutableInteractionSource() }
                Surface(color = fill, shape = TyuShapes.Control, border = BorderStroke(TyuDimens.Tiny / 4, stroke),
                    modifier = Modifier.tyuPressScale(interaction)) {
                    Box(Modifier.selectable(active, interaction, indication = null,
                        role = Role.RadioButton, onClick = { onSelect(choice) })
                        .heightIn(min = TyuDimens.Touch)
                        .padding(horizontal = TyuDimens.Gap, vertical = TyuDimens.Medium), contentAlignment = Alignment.Center) {
                        Text(choice, color = if (active) TyuColors.Primary else TyuColors.Secondary,
                            style = MaterialTheme.typography.bodyMedium)
                    }
                }
            }
        }
    }
}
