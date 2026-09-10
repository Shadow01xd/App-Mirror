package com.tyu.app.feature.camera

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
fun CameraScreen(computer: String, onStart: () -> Unit, onBack: () -> Unit,
    initial: CameraUiState = CameraUiState()) {
    var lens by rememberSaveable { mutableStateOf(initial.lens) }
    var quality by rememberSaveable { mutableStateOf(initial.quality) }
    TyuPage(onBack = onBack, bottom = {
        TyuButton("Activar cámara", onStart, Modifier.fillMaxWidth().padding(vertical = TyuDimens.Medium), CameraIcon)
    }) {
        TyuConnectionBanner(computer)
        TyuHeading("Cámara TYU", "Permite utilizar este teléfono como cámara de $computer.")
        TyuCameraPreview()
        TyuOptions("CÁMARA", listOf("Principal", "Frontal", "Ultra gran angular"), lens, { lens = it })
        TyuOptions("CALIDAD", listOf("Automática", "1080p", "720p"), quality, { quality = it })
    }
}
