package com.tyu.app.feature.monitor

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import com.tyu.app.ui.components.*
import com.tyu.app.ui.icons.*
import com.tyu.app.ui.theme.*

@Composable
fun MonitorActiveScreen(computer: String, onStop: () -> Unit, onBack: () -> Unit) {
    var controls by rememberSaveable { mutableStateOf(true) }
    val compact = LocalConfiguration.current.screenHeightDp < 500
    Surface(Modifier.fillMaxSize(), color = TyuColors.Background) {
        Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)
            .clickable(role = Role.Button, onClickLabel = "Mostrar u ocultar controles") { controls = !controls }
            .padding(TyuDimens.Page), horizontalAlignment = Alignment.CenterHorizontally) {
            AnimatedVisibility(controls, enter = fadeIn(tween(TyuMotion.State)), exit = fadeOut(tween(TyuMotion.Press))) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically) {
                    Text("TYU", color = TyuColors.Primary, style = MaterialTheme.typography.displaySmall)
                    TyuIconButton(TyuIcons.Close, "Volver al dispositivo", onBack)
                }
            }
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Column(Modifier.verticalScroll(rememberScrollState()).padding(vertical = TyuDimens.Gap),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(if (compact) TyuDimens.Small else TyuDimens.Gap)) {
                    if (!compact) TyuDeviceStage(Modifier.fillMaxWidth().height(TyuDimens.HeroIcon * 1.4f))
                    else Icon(TyuIcons.Desktop, null, Modifier.size(TyuDimens.CompactHeroIcon), tint = TyuColors.Muted)
                    Text("Monitor TYU activo", style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
                    TyuStatusDot(computer)
                    Text("Vista de pantalla · sin señal de video", color = TyuColors.Secondary,
                        style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
                }
            }
            AnimatedVisibility(controls, enter = fadeIn(tween(TyuMotion.State)), exit = fadeOut(tween(TyuMotion.Press))) {
                TyuButton("Detener monitor", onStop, icon = TyuIcons.Stop, primary = false)
            }
        }
    }
}
