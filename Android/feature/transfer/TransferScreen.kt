package com.tyu.app.feature.transfer

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
fun TransferScreen(computer: String, onSend: () -> Unit, onQueue: () -> Unit,
    hasQueue: Boolean, onBack: () -> Unit) {
    var category by rememberSaveable { mutableStateOf("Fotos") }
    var selected by rememberSaveable { mutableStateOf(true) }
    var showPicker by rememberSaveable { mutableStateOf(false) }
    TyuPage(onBack = onBack) {
        TyuHeading("Enviar a $computer", "Algo que vale la pena compartir.")
        TyuOptions("TIPO DE ARCHIVO", listOf("Fotos", "Videos", "Archivos"), category,
            { category = it; selected = false })
        TyuCard {
            Icon(TransferIcon, null, tint = TyuColors.Primary, modifier = Modifier.size(TyuDimens.Large))
            Text(if (selected) "3 archivos seleccionados" else "Nada seleccionado",
                style = MaterialTheme.typography.titleMedium)
            Text(if (selected) "foto.jpg · video.mp4 · documento.pdf\n53.2 MB en total"
                else "Prepara una selección para tu equipo.",
                color = TyuColors.Secondary, style = MaterialTheme.typography.bodyMedium)
            TyuSecondaryButton("Elegir archivos", { showPicker = true }, Modifier.fillMaxWidth(), StorageIcon)
        }
        TyuButton("Enviar archivos", onSend, Modifier.fillMaxWidth(), TransferIcon, enabled = selected)
        if (hasQueue) TyuSecondaryButton("Ver cola", onQueue, Modifier.fillMaxWidth())
        TyuFootnote("Los archivos de esta selección son ejemplos. No se accede a tu almacenamiento.")
    }
    if (showPicker) TyuBottomSheet("Seleccionar $category", { showPicker = false }) {
        Text("foto.jpg\nvideo.mp4\ndocumento.pdf", style = MaterialTheme.typography.bodyLarge)
        TyuButton("Seleccionar ejemplos", { selected = true; showPicker = false }, Modifier.fillMaxWidth())
    }
}
