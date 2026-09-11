package com.tyu.app.feature.sessions

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

@Composable
fun SessionsScreen(state: SessionsUiState, onOpen: (String) -> Unit, onBack: () -> Unit) {
    TyuPage(onBack = onBack) {
        TyuHeading("Actividad actual", "Lo que está pasando entre tus dispositivos.")
        if (state.sessions.none { it.active }) {
            TyuHero(TyuIcons.Activity, "Todo en pausa", "Activa un modo o servicio para verlo aquí.")
        }
        state.sessions.forEach { session ->
            TyuFeatureCard(session.title, if (session.active) session.detail else "Inactivo",
                when (session.title) {
                    "Cámara" -> CameraIcon
                    "Micrófono" -> MicrophoneIcon
                    "Almacenamiento" -> StorageIcon
                    "Transferencia" -> TransferIcon
                    else -> TyuIcons.Devices
                }, { onOpen(session.title) }, session.active)
        }
    }
}
