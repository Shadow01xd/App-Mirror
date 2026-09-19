package com.tyu.app.feature.storage

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
fun StorageScreen(computer: String, state: StorageUiState, onEnabled: (Boolean) -> Unit, onBack: () -> Unit) {
    TyuPage(onBack = onBack) {
        TyuConnectionBanner(computer)
        TyuHeading("Acceso al almacenamiento", "Abre las carpetas de este teléfono desde el Explorador de Windows.")
        TyuCard { TyuToggle("Compartir almacenamiento", "Tú decides qué compartir.",
            state.enabled, onEnabled, StorageIcon) }
        TyuCard {
            Text("TYU: ${android.os.Build.MODEL}", style = MaterialTheme.typography.titleMedium)
            Text("DCIM · Pictures · Download · Documents · Music · Android", color = TyuColors.Secondary)
        }
        TyuFootnote("Solo tu PC vinculado accede por la red local mientras compartes. Android mantiene protegidos los datos privados de otras apps, incluidas sus carpetas Android/data y Android/obb.")
    }
}
