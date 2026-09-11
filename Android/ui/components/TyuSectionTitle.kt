package com.tyu.app.ui.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.tyu.app.ui.theme.*

@Composable
fun TyuSectionTitle(title: String) {
    Text(title, Modifier.fillMaxWidth(), color = TyuColors.Secondary,
        style = MaterialTheme.typography.labelMedium)
}
