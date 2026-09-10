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
fun CameraActiveScreen(computer: String, onStop: () -> Unit, onBack: () -> Unit) {
    TyuActiveService("Cámara activa", "Cámara TYU disponible para $computer.", computer,
        CameraIcon, onStop, onBack) {
        TyuFootnote("Vista simulada · sin captura de imagen")
    }
}
