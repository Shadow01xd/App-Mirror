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
    var categories by rememberSaveable { mutableStateOf(listOf("Imágenes", "Documentos", "Descargas")) }
    TyuPage(onBack = onBack) {
        TyuConnectionBanner(computer)
        TyuHeading("Acceso al almacenamiento", "Elige qué categorías estarán disponibles en tu PC.")
        TyuCard { TyuToggle("Compartir almacenamiento", "Tú decides qué compartir.",
            state.enabled, onEnabled, StorageIcon) }
        TyuSectionTitle("CATEGORÍAS")
        TyuCard {
            StorageUiState.categories.forEach { name ->
                TyuToggle(name, "", name in categories, { selected ->
                    categories = ArrayList(if (selected) categories + name else categories - name)
                }, enabled = state.enabled)
            }
        }
        TyuFootnote("Las categorías son ilustrativas. No se leen carpetas ni archivos del teléfono.")
    }
}
