package com.tyu.app.feature.home.components

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

@Composable
fun ActiveSessionCard(count: Int, onClick: () -> Unit) {
    TyuFeatureCard("Actividad actual",
        if (count == 1) "1 servicio disponible o activo" else "$count servicios disponibles o activos",
        TyuIcons.Activity, onClick)
}
