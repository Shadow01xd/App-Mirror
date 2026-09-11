package com.tyu.app.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.tyu.app.ui.theme.*

@Composable
fun TyuCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Surface(modifier.fillMaxWidth(), shape = TyuShapes.Card, color = TyuColors.Surface,
        ) {
        Column(Modifier.padding(TyuDimens.Page), verticalArrangement = Arrangement.spacedBy(TyuDimens.Gap),
            content = content)
    }
}
