package com.tyu.app.feature.transfer

import android.content.Intent
import android.net.Uri
import android.widget.Toast
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
    fun open(file: BypassFile) {
        val uri = file.saved?.let(Uri::parse) ?: return
        val intent = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, context.contentResolver.getType(uri) ?: "*/*")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        runCatching { context.startActivity(intent) }
            .onFailure { Toast.makeText(context, "No hay ninguna app para abrir este archivo", Toast.LENGTH_SHORT).show() }
    }
    // Received files come first: that is what someone opens the screen to find.
    val ordered = files.sortedByDescending { it.incoming && it.saved != null }
    TyuPage(onBack = onBack) {
        TyuHeading("Archivos con $computer",
            "Envía desde aquí, o recibe lo que te manden desde el PC. Todo viaja por tu red local.")
        TyuButton("Enviar archivos al PC", { if (onSelected != null) picker.launch(arrayOf("*/*")) else onSend() },
            Modifier.fillMaxWidth(), TransferIcon)
        TyuFootnote("Lo que recibas se guarda en Descargas/TYU de este teléfono.")
        if (files.isEmpty()) {
            TyuFootnote("Todavía no hay transferencias.")
        }
        ordered.forEach { file ->
            val active = file.status in listOf("Enviando", "Transfiriendo", "Recibiendo")
            TyuCard {
                Text((if (file.incoming) "↓ " else "↑ ") + file.name, style = MaterialTheme.typography.titleMedium)
                Text(
                    (if (file.incoming) "Del PC" else "Al PC") + " · " + file.status +
                        if (file.total > 0) " · " + android.text.format.Formatter.formatShortFileSize(context, file.total) else "",
                    color = TyuColors.Secondary)
                if (active) {
                    LinearProgressIndicator(
                        progress = { if (file.total > 0) (file.bytes.toFloat() / file.total).coerceIn(0f, 1f) else 0f },
                        modifier = Modifier.fillMaxWidth())
                    if (file.path != null) TyuSecondaryButton("Cancelar", { onCancel(file.id) }, Modifier.fillMaxWidth())
                }
                if (file.saved != null) TyuSecondaryButton("Abrir", { open(file) }, Modifier.fillMaxWidth())
            }
        }
    }
}
