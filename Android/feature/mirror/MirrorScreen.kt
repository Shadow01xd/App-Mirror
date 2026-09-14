package com.tyu.app.feature.mirror

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

import com.tyu.app.model.UiMode

@Composable
fun MirrorScreen(computer: String, state: MirrorUiState, onStart: () -> Unit, onBack: () -> Unit,
    controlEnabled: Boolean = false, onEnableControl: () -> Unit = {}) {
    TyuPage(onBack = onBack, spread = true, bottom = {
        TyuButton(if (state == MirrorUiState.Connecting) "Preparando…" else "Transmitir pantalla",
            onStart, Modifier.fillMaxWidth().padding(vertical = TyuDimens.Medium), UiMode.Mirror.icon(), enabled = state == MirrorUiState.Idle)
    }) {
        TyuConnectionBanner(computer)
        TyuHeading("Espejo", "Comparte la pantalla de este dispositivo con $computer.")
        TyuDeviceStage(Modifier.fillMaxWidth().height(TyuDimens.HeroIcon * 1.4f), toPhone = false)
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TyuDimens.Page)) {
            TyuDirectionStrip("teléfono", "PC")
            if (state == MirrorUiState.Connecting) {
                TyuProgressBar(null)
                TyuFootnote("Preparando la pantalla…")
            }
            TyuControlStatus(controlEnabled, onEnableControl)
        }
    }
}

/** Whether the PC can control this phone, with the one-time Accessibility step when it cannot. */
@Composable
fun TyuControlStatus(enabled: Boolean, onEnable: () -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TyuDimens.Small)) {
        TyuFootnote(if (enabled) "Control desde la PC: activado"
            else "Control desde la PC: desactivado · activa \"TYU control táctil\" en Accesibilidad")
        if (!enabled) TyuSecondaryButton("Activar control desde la PC", onEnable, Modifier.fillMaxWidth())
    }
}
