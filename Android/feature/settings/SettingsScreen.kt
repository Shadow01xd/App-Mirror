package com.tyu.app.feature.settings

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
fun SettingsScreen(connected: Boolean, onPermissions: () -> Unit, onDevice: () -> Unit, onBack: () -> Unit,
    initial: SettingsUiState = SettingsUiState()) {
    var nearby by rememberSaveable { mutableStateOf(initial.nearby) }
    var wifi by rememberSaveable { mutableStateOf(initial.wifi) }
    var bluetooth by rememberSaveable { mutableStateOf(initial.bluetooth) }
    var direct by rememberSaveable { mutableStateOf(initial.wifiDirect) }
    var confirm by rememberSaveable { mutableStateOf(initial.confirmFiles) }
    var quality by rememberSaveable { mutableStateOf("Automática") }
    var fps by rememberSaveable { mutableStateOf("60") }
    var orientation by rememberSaveable { mutableStateOf("Automática") }
    var sheet by rememberSaveable { mutableStateOf("") }
    TyuPage(onBack = onBack) {
        TyuHeading("Configuración", "TYU, a tu manera.")
        TyuSectionTitle("CONEXIÓN")
        TyuCard {
            TyuToggle("Detección cercana", "Encontrar equipos alrededor", nearby, { nearby = it }, TyuIcons.Devices)
            TyuToggle("Wi-Fi", "Preferir red local", wifi, { wifi = it }, TyuIcons.Wifi)
            TyuToggle("Bluetooth LE", "Detección de proximidad", bluetooth, { bluetooth = it }, TyuIcons.Bluetooth)
            TyuToggle("Wi-Fi Direct", "Enlace entre dispositivos", direct, { direct = it }, TyuIcons.Wifi)
        }
        TyuSectionTitle("PRIVACIDAD")
        TyuFeatureCard("Dispositivos confiables", if (connected) "1 equipo" else "Ningún equipo",
            TyuIcons.Shield, { if (connected) onDevice() else sheet = "Dispositivos confiables" })
        TyuFeatureCard("Permisos", "Revisar categorías", TyuIcons.Shield, onPermissions)
        TyuSectionTitle("TRANSFERENCIAS")
        TyuFeatureCard("Recibidos", "Ver actividad reciente", StorageIcon, { sheet = "Recibidos" })
        TyuCard { TyuToggle("Confirmación de archivos", "Preguntar antes de recibir", confirm, { confirm = it }) }
        TyuSectionTitle("PANTALLA")
        TyuOptions("Calidad", listOf("Automática", "Calidad", "Baja latencia"), quality, { quality = it })
        TyuOptions("FPS", listOf("30", "60", "120"), fps, { fps = it })
        TyuOptions("Orientación", listOf("Automática", "Horizontal", "Vertical"), orientation, { orientation = it })
        TyuSectionTitle("ACERCA DE")
        TyuCard {
            Text("TYU", color = TyuColors.Primary, style = MaterialTheme.typography.displaySmall)
            Text("Versión 0.1.0", style = MaterialTheme.typography.bodyLarge)
            Text("Frontend visual · Kotlin + Compose", color = TyuColors.Secondary,
                style = MaterialTheme.typography.bodySmall)
        }
    }
    if (sheet.isNotEmpty()) TyuBottomSheet(sheet, { sheet = "" }) {
        Text(if (sheet == "Recibidos") "Todavía no hay archivos recibidos." else "Conecta un dispositivo para verlo aquí.",
            style = MaterialTheme.typography.bodyLarge)
        TyuSecondaryButton("Cerrar", { sheet = "" }, Modifier.fillMaxWidth())
    }
}
