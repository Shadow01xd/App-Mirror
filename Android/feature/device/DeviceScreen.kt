package com.tyu.app.feature.device

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
fun DeviceScreen(state: DeviceUiState, onForget: () -> Unit, onStorage: (Boolean) -> Unit,
    onCamera: (Boolean) -> Unit, onMicrophone: (Boolean) -> Unit, onBack: () -> Unit,
    onDisconnect: (() -> Unit)? = null) {
    var confirm by rememberSaveable { mutableStateOf(false) }
    TyuPage(onBack = onBack) {
        TyuHeading(state.device.name, "Un equipo en tu espacio TYU.")
        TyuStatusDot("Conectado")
        TyuCard {
            listOf("Equipo" to state.device.computerName, "Conexión" to state.device.connectionQuality,
                "IP" to state.device.ip, "Proximidad" to if (state.device.nearby) "Cerca" else "Fuera de alcance")
                .forEach { (label, value) ->
                    Column(verticalArrangement = Arrangement.spacedBy(TyuDimens.Tiny)) {
                        Text(label, color = TyuColors.Secondary, style = MaterialTheme.typography.bodySmall)
                        Text(value, style = MaterialTheme.typography.bodyLarge)
                    }
                }
        }
        TyuSectionTitle("SERVICIOS DISPONIBLES")
        TyuCard {
            TyuToggle("Almacenamiento", "Compartir categorías", state.storageAllowed, onStorage, StorageIcon)
            TyuToggle("Cámara", "Cámara TYU", state.cameraAllowed, onCamera, CameraIcon)
            TyuToggle("Micrófono", "Micrófono TYU", state.microphoneAllowed, onMicrophone, MicrophoneIcon)
        }
        TyuSecondaryButton("Olvidar dispositivo", { confirm = true }, Modifier.fillMaxWidth(), TyuIcons.Close)
        if (onDisconnect != null) TyuSecondaryButton("Desconectar", onDisconnect, Modifier.fillMaxWidth())
    }
    if (confirm) TyuDialog("¿Olvidar ${state.device.name}?",
        if (onDisconnect == null) "Se cerrarán las sesiones de esta demostración. Podrás volver a agregar el equipo."
        else "Se cerrará la conexión y se revocará la confianza en este equipo.",
        "Olvidar", { confirm = false; onForget() }, { confirm = false })
}
