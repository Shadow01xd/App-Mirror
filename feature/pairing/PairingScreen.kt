package com.tyu.app.feature.pairing

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

import androidx.compose.foundation.BorderStroke
import com.tyu.app.model.UiDevice

@Composable
fun PairingScreen(state: PairingUiState, device: UiDevice, onContinue: () -> Unit,
    onCancel: () -> Unit) {
    TyuPage(onBack = onCancel, spread = true) {
        when (state) {
            PairingUiState.Qr -> {
                TyuHeading("Conecta con un código", "Abre TYU en tu PC y busca su código QR.")
                Surface(Modifier.size(TyuDimens.HeroIcon * 1.65f),
                    color = TyuColors.Surface, shape = TyuShapes.Card,
                    border = BorderStroke(TyuDimens.Tiny / 2, TyuColors.Primary)) {
                    Box(contentAlignment = Alignment.Center) {
                        Icon(TyuIcons.Qr, "Marco del escáner QR", Modifier.size(TyuDimens.HeroIcon),
                            tint = TyuColors.Secondary)
                    }
                }
                Column(verticalArrangement = Arrangement.spacedBy(TyuDimens.Gap)) {
                    TyuFootnote("Vista previa del escáner · cámara desactivada")
                    TyuButton("Continuar", onContinue, Modifier.fillMaxWidth(), TyuIcons.Arrow)
                    TyuSecondaryButton("Cancelar", onCancel, Modifier.fillMaxWidth())
                }
            }
            PairingUiState.Connecting -> {
                TyuHero(TyuIcons.Devices, "Creando el enlace", "Conectando con ${device.computerName}…")
                TyuProgressBar(null)
                TyuSecondaryButton("Cancelar", onCancel, Modifier.fillMaxWidth())
            }
            PairingUiState.Error -> {
                TyuHero(TyuIcons.Desktop, "No pudimos conectar",
                    "No se pudo conectar con ${device.name}.", error = true)
                TyuStatusDot("Conexión interrumpida", active = false, error = true)
                Column(verticalArrangement = Arrangement.spacedBy(TyuDimens.Gap)) {
                    TyuButton("Reintentar", onContinue, Modifier.fillMaxWidth(), TyuIcons.Refresh)
                    TyuSecondaryButton("Elegir otro dispositivo", onCancel, Modifier.fillMaxWidth())
                }
            }
        }
    }
}
