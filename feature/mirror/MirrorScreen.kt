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
fun MirrorScreen(computer: String, state: MirrorUiState, onStart: () -> Unit, onBack: () -> Unit) {
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
        }
    }
}
