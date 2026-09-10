package com.tyu.app.ui.components

import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import com.tyu.app.ui.theme.*

@Composable
fun TyuIconButton(icon: ImageVector, description: String, onClick: () -> Unit) {
    IconButton(onClick, Modifier.size(TyuDimens.Touch)) {
        Icon(icon, description, tint = TyuColors.Secondary, modifier = Modifier.size(TyuDimens.Icon))
    }
}
