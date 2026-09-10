package com.tyu.app.feature.bypass

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

import com.tyu.app.model.UiFeature
import com.tyu.app.model.UiMode
import com.tyu.app.feature.home.components.FeatureGrid

@Composable
fun BypassScreen(computer: String, cameraActive: Boolean, microphoneActive: Boolean,
    storageEnabled: Boolean, onFeature: (UiFeature) -> Unit, onBack: () -> Unit) {
    TyuPage(onBack = onBack) {
        TyuConnectionBanner(computer)
        TyuHeading("Bypass", "Conecta ambos dispositivos sin compartir ni extender pantallas.")
        TyuDirectionStrip("teléfono", "PC", bidirectional = true)
        TyuStatusDot("Conexión de servicios activa")
        TyuSectionTitle("TUS SERVICIOS")
        FeatureGrid(cameraActive, microphoneActive, storageEnabled, onFeature)
        TyuFootnote("Solo lo que necesitas. Ninguna pantalla compartida.")
    }
}
