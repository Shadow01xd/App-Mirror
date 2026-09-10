package com.tyu.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.*
import com.tyu.app.ui.theme.*

@Composable
fun TyuStatusDot(text: String, active: Boolean = true, error: Boolean = false) {
    Row(Modifier.semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(TyuDimens.Small)) {
        Box(Modifier.size(TyuDimens.Small).background(
            if (error) TyuColors.Error else if (active) TyuColors.Success else TyuColors.Muted, CircleShape))
        Text(text, color = if (error) TyuColors.Error else if (active) TyuColors.Success else TyuColors.Secondary,
            style = MaterialTheme.typography.bodyMedium)
    }
}
