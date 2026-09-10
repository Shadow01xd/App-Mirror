package com.tyu.app.ui.components

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.tyu.app.ui.theme.*

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TyuBottomSheet(title: String, onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = TyuColors.Surface) {
        Column(Modifier.fillMaxWidth().padding(TyuDimens.Page),
            verticalArrangement = Arrangement.spacedBy(TyuDimens.Gap)) {
            Text(title, style = MaterialTheme.typography.titleLarge)
            content()
        }
    }
}
