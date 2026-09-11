package com.tyu.app.feature.monitor

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
fun MonitorScreen(computer: String, onStart: () -> Unit, onBack: () -> Unit,
    initial: MonitorUiState = MonitorUiState()) {
    var desktop by rememberSaveable { mutableStateOf(initial.desktopMode) }
    var orientation by rememberSaveable { mutableStateOf(initial.orientation) }
    var quality by rememberSaveable { mutableStateOf(initial.quality) }
    TyuPage(onBack = onBack, bottom = {
        TyuButton("Usar como monitor", onStart, Modifier.fillMaxWidth().padding(vertical = TyuDimens.Medium), TyuIcons.Desktop)
    }) {
        TyuConnectionBanner(computer)
        TyuHeading("Monitor", "Usar este dispositivo como segunda pantalla de $computer.")
        TyuDirectionStrip("PC", "teléfono")
        TyuOptions("MODO", listOf("Extender escritorio", "Duplicar pantalla"), desktop, { desktop = it })
        TyuOptions("ORIENTACIÓN", listOf("Automática", "Horizontal", "Vertical"), orientation, { orientation = it })
        TyuOptions("CALIDAD", listOf("Automática", "Calidad", "Baja latencia"), quality, { quality = it })
    }
}
