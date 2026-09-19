package com.tyu.app.feature.transfer

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import com.tyu.app.backend.BypassFile
import com.tyu.app.ui.components.*
import com.tyu.app.ui.icons.*
import com.tyu.app.ui.theme.*

@Composable
fun TransferScreen(computer: String, onSend: () -> Unit, onQueue: () -> Unit,
    hasQueue: Boolean, onBack: () -> Unit, files: List<BypassFile> = emptyList(),
    onSelected: ((List<Uri>) -> Unit)? = null, onCancel: (String) -> Unit = {}) {
    val context = LocalContext.current
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isNotEmpty()) onSelected?.invoke(uris)
    }
    TyuPage(onBack = onBack) {
        TyuHeading("Enviar a $computer", "Elige archivos del teléfono para copiarlos a Documentos/TYU en tu PC.")
        TyuButton("Elegir archivos y enviar", { if (onSelected != null) picker.launch(arrayOf("*/*")) else onSend() }, Modifier.fillMaxWidth(), TransferIcon)
        if (files.isEmpty()) TyuFootnote("Todavía no hay transferencias. Los archivos viajan por tu red local, sin Internet.")
        files.forEach { file ->
            TyuCard {
                Text(file.name, style = MaterialTheme.typography.titleMedium)
                Text(file.status + " · " + android.text.format.Formatter.formatShortFileSize(context,file.bytes) + " / " + android.text.format.Formatter.formatShortFileSize(context,file.total),color=TyuColors.Secondary)
                if (file.status in listOf("Enviando","Transfiriendo","Recibiendo")) {
                    LinearProgressIndicator(progress = { if(file.total>0) (file.bytes.toFloat()/file.total).coerceIn(0f,1f) else 0f },modifier=Modifier.fillMaxWidth())
                    if (file.path != null) TyuSecondaryButton("Cancelar", { onCancel(file.id) },Modifier.fillMaxWidth())
                }
            }
        }
    }
}