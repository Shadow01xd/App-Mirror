package com.tyu.app.feature.nearby

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

import com.tyu.app.mock.MockDevices
import com.tyu.app.model.UiDevice

@Composable
fun NearbyScreen(state: NearbyUiState, onSearch: () -> Unit, onPair: (UiDevice) -> Unit,
    onQr: () -> Unit, onBack: () -> Unit, onSettings: () -> Unit) {
    TyuPage(onBack = onBack, onSettings = onSettings, spread = state != NearbyUiState.Found) {
        if (state == NearbyUiState.Found) {
            TyuHeading("Tu próximo enlace", "Elige el equipo que quieres conectar.")
            TyuSectionTitle("DISPOSITIVOS CERCANOS · ${MockDevices.nearby.size}")
            MockDevices.nearby.forEach { device -> TyuDeviceCard(device, { onPair(device) }) }
            TyuSecondaryButton("Buscar de nuevo", onSearch, Modifier.fillMaxWidth(), TyuIcons.Refresh)
        } else {
            TyuHero(TyuIcons.Desktop,
                if (state == NearbyUiState.Searching) "Buscando dispositivos" else "Tu PC, a un paso",
                if (state == NearbyUiState.Searching) "Buscando dispositivos cercanos…"
                else "No se detectan dispositivos.\nAgrega un equipo para empezar.")
            if (state == NearbyUiState.Searching) {
                Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TyuDimens.Gap)) {
                    TyuProgressBar(null)
                    TyuFootnote("Buscando un nuevo espacio en común")
                }
            } else {
                TyuButton("Agregar dispositivo", onSearch, Modifier.fillMaxWidth(), TyuIcons.Add)
            }
        }
        Column(Modifier.fillMaxWidth().padding(top = TyuDimens.Gap),
            verticalArrangement = Arrangement.spacedBy(TyuDimens.Medium)) {
            TyuFootnote("¿No encuentras tu dispositivo?")
            TyuSecondaryButton("Conectar mediante QR", onQr, Modifier.fillMaxWidth(), TyuIcons.Qr)
        }
    }
}
