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
fun MirrorActiveScreen(computer: String, onStop: () -> Unit, onBack: () -> Unit,
    status: String = "Transmitiendo a $computer", controlEnabled: Boolean = false, onEnableControl: () -> Unit = {}) {
    TyuActiveService("Espejo activo", status, computer,
        UiMode.Mirror.icon(), onStop, onBack) {
        TyuFootnote("teléfono → PC")
        TyuControlStatus(controlEnabled, onEnableControl)
    }
}
