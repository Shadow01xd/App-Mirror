package com.tyu.app.ui.components

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

import androidx.compose.ui.graphics.vector.ImageVector

@Composable
fun TyuActiveService(title: String, subtitle: String, computer: String, icon: ImageVector,
    onStop: () -> Unit, onBack: () -> Unit, content: @Composable () -> Unit = {}) {
    TyuPage(onBack = onBack, spread = true, bottom = {
        TyuSecondaryButton("Detener", onStop, Modifier.fillMaxWidth().padding(vertical = TyuDimens.Medium), TyuIcons.Stop)
    }) {
        TyuConnectionBanner(computer)
        Column(horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(TyuDimens.Page)) {
            TyuHero(icon, title, subtitle)
            TyuStatusDot("Activo")
            content()
        }
    }
}
