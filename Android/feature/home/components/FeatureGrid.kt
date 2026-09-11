package com.tyu.app.feature.home.components

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

@Composable
fun FeatureGrid(cameraActive: Boolean, microphoneActive: Boolean, storageEnabled: Boolean,
    onFeature: (UiFeature) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(TyuDimens.Medium)) {
        Box(Modifier.fillMaxWidth().tyuEnter(0)) {
            TyuFeatureCard("Almacenamiento", if (storageEnabled) "Disponible" else "Desactivado",
                StorageIcon, { onFeature(UiFeature.Storage) }, storageEnabled)
        }
        Box(Modifier.fillMaxWidth().tyuEnter(1)) {
            TyuFeatureCard("Cámara", if (cameraActive) "Activa" else "Desactivada",
                CameraIcon, { onFeature(UiFeature.Camera) }, cameraActive)
        }
        Box(Modifier.fillMaxWidth().tyuEnter(2)) {
            TyuFeatureCard("Micrófono", if (microphoneActive) "Activo" else "Desactivado",
                MicrophoneIcon, { onFeature(UiFeature.Microphone) }, microphoneActive)
        }
        Box(Modifier.fillMaxWidth().tyuEnter(3)) {
            TyuFeatureCard("Enviar archivos", "Fotos, videos y documentos",
                TransferIcon, { onFeature(UiFeature.Transfer) })
        }
    }
}
