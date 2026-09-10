package com.tyu.app.feature.home

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

import com.tyu.app.feature.home.components.*
import com.tyu.app.model.UiMode

@Composable
fun HomeScreen(state: HomeUiState, onMode: (UiMode) -> Unit, onOpenMode: () -> Unit,
    onDetails: () -> Unit, onSettings: () -> Unit, onSessions: () -> Unit) {
    TyuPage(onSettings = onSettings, onSessions = onSessions,
        bottom = { TyuModeSelector(state.mode, onMode) }) {
        ConnectedDeviceHeader(state.device, onDetails, toPhone = state.mode != UiMode.Mirror)
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TyuDimens.Gap)) {
            Text(when (state.mode) {
                UiMode.Monitor -> "Más espacio para lo que estás haciendo."
                UiMode.Mirror -> "Tu pantalla, también en tu PC."
                UiMode.Bypass -> "Tus servicios, entre ambos dispositivos."
            }, color = TyuColors.Secondary, style = MaterialTheme.typography.bodyMedium)
            TyuButton("Abrir ${state.mode.title}", onOpenMode, Modifier.fillMaxWidth(), state.mode.icon())
            Text(state.mode.direction, Modifier.fillMaxWidth(),
                textAlign = TextAlign.Center, color = TyuColors.Secondary, style = MaterialTheme.typography.bodySmall)
        }
    }
}
