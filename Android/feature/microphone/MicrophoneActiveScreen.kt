package com.tyu.app.feature.microphone

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

import androidx.compose.foundation.background
import androidx.compose.ui.draw.clip

@Composable
fun MicrophoneActiveScreen(computer: String, onStop: () -> Unit, onBack: () -> Unit, live: Boolean = false) {
    TyuActiveService("Micrófono activo", "Tu voz, en $computer.", computer,
        MicrophoneIcon, onStop, onBack) {
        if (!live) TyuAudioStage(active = true)
        TyuFootnote(if (live) "Audio 48 kHz en directo · TYU Desktop te indica qué micrófono elegir en el PC" else "Nivel ilustrativo · sin captura de audio")
    }
}
