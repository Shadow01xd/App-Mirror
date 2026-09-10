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

@Composable
fun MicrophoneScreen(computer: String, onStart: () -> Unit, onBack: () -> Unit,
    initial: MicrophoneUiState = MicrophoneUiState()) {
    var noise by rememberSaveable { mutableStateOf(initial.noiseReduction) }
    TyuPage(onBack = onBack, bottom = {
        TyuButton("Activar micrófono", onStart, Modifier.fillMaxWidth().padding(vertical = TyuDimens.Medium), MicrophoneIcon)
    }) {
        TyuConnectionBanner(computer)
        TyuHeading("Micrófono TYU", "Permite utilizar este teléfono como micrófono de $computer.")
        TyuAudioStage()
        TyuCard {
            Text(initial.source, style = MaterialTheme.typography.titleMedium)
            Text("Fuente de audio", color = TyuColors.Secondary, style = MaterialTheme.typography.bodyMedium)
        }
        TyuCard { TyuToggle("Reducción de ruido", "Una voz más clara.", noise, { noise = it }) }
    }
}
