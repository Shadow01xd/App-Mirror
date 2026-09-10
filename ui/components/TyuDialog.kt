package com.tyu.app.ui.components

import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import com.tyu.app.ui.theme.*

@Composable
fun TyuDialog(title: String, message: String, confirmText: String,
    onConfirm: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(onDismissRequest = onDismiss, title = { Text(title) }, text = { Text(message) },
        confirmButton = { TextButton(onConfirm) { Text(confirmText, color = TyuColors.Primary) } },
        dismissButton = { TextButton(onDismiss) { Text("Cancelar", color = TyuColors.Secondary) } },
        containerColor = TyuColors.Surface, shape = TyuShapes.Card)
}
