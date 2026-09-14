package com.tyu.app.feature.monitor

import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import com.tyu.app.ui.components.*
import com.tyu.app.ui.icons.*
import com.tyu.app.ui.theme.*

// Only settings that actually do something are offered: orientation locks this phone's screen
// while it shows the PC. Extend/duplicate needs a Windows virtual display driver that does not
// exist yet, and quality is decided by the PC encoder.
@Composable
fun MonitorScreen(computer: String, onStart: () -> Unit, onBack: () -> Unit,
    initial: MonitorUiState = MonitorUiState(), onOrientation: (String) -> Unit = {}) {
    TyuPage(onBack = onBack, bottom = {
        TyuButton("Usar como monitor", onStart, Modifier.fillMaxWidth().padding(vertical = TyuDimens.Medium), TyuIcons.Desktop)
    }) {
        TyuConnectionBanner(computer)
        TyuHeading("Monitor", "Ver la pantalla de $computer en este dispositivo.")
        TyuDirectionStrip("PC", "teléfono")
        TyuOptions("ORIENTACIÓN", listOf("Automática", "Horizontal", "Vertical"), initial.orientation, onOrientation)
    }
}
