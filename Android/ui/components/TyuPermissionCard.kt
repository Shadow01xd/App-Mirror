package com.tyu.app.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.vector.ImageVector

@Composable
fun TyuPermissionCard(title: String, subtitle: String, icon: ImageVector, checked: Boolean,
    onCheckedChange: (Boolean) -> Unit) {
    TyuCard { TyuToggle(title, subtitle, checked, onCheckedChange, icon) }
}
