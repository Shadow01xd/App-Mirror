package com.tyu.app.feature.transfer

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import com.tyu.app.ui.components.*
import com.tyu.app.ui.icons.*
import com.tyu.app.ui.theme.*

import com.tyu.app.mock.MockTransfers
import kotlin.math.roundToInt

@Composable
fun TransferProgressScreen(computer: String, state: TransferUiState, onComplete: () -> Unit,
    onCancel: () -> Unit, onBack: () -> Unit) {
    TyuPage(onBack = onBack) {
        TyuHeading(if (state.finished) "Todo en su lugar" else if (state.active) "Enviando a $computer"
            else "Transferencia cancelada", "3 archivos · 53.2 MB")
        TyuStatusDot(if (state.finished) "Completado" else if (state.active) "Transferencia activa" else "Cancelada",
            active = state.active || state.finished)
        MockTransfers.queue.forEach { item ->
            val progress = if (state.finished) 1f else item.progress
            TyuCard {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(TyuDimens.Small)) {
                    Text(item.name, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
                    Text(if (!state.active && !state.finished) "—" else progress?.let { "${(it * 100).roundToInt()} %" } ?: "Esperando",
                        style = MaterialTheme.typography.bodySmall, color = TyuColors.Secondary)
                }
                Text(item.size, style = MaterialTheme.typography.bodySmall, color = TyuColors.Secondary)
                TyuProgressBar(progress ?: 0f)
            }
        }
        if (state.active) {
            TyuButton("Completar vista previa", onComplete, Modifier.fillMaxWidth(), TyuIcons.Check)
            TyuSecondaryButton("Cancelar transferencia", onCancel, Modifier.fillMaxWidth(), TyuIcons.Close)
        } else TyuButton("Volver", onBack, Modifier.fillMaxWidth())
    }
}
