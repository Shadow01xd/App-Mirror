package com.tyu.app.feature.monitor

import android.app.Activity
import android.content.pm.ActivityInfo
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import com.tyu.app.ui.components.*
import com.tyu.app.ui.icons.*
import com.tyu.app.ui.theme.*
import kotlinx.coroutines.channels.ReceiveChannel

@Composable
fun MonitorActiveScreen(computer: String, onStop: () -> Unit, onBack: () -> Unit,
    frames: ReceiveChannel<ByteArray>? = null, orientation: String = "Automática", onNeedKeyframe: () -> Unit = {}) {
    var controls by rememberSaveable { mutableStateOf(true) }
    val compact = LocalConfiguration.current.screenHeightDp < 500
    val activity = LocalContext.current as? Activity
    DisposableEffect(activity, orientation) {
        activity?.requestedOrientation = when (orientation) {
            "Horizontal" -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
            "Vertical" -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_PORTRAIT
            else -> ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
        }
        onDispose { activity?.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED }
    }
    if (frames != null) {
        // Live PC screen: edge to edge, system bars hidden, controls fade in on tap.
        val view = LocalView.current
        DisposableEffect(view) {
            val window = (view.context as? Activity)?.window
            val insets = window?.let { WindowCompat.getInsetsController(it, view) }
            insets?.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            insets?.hide(WindowInsetsCompat.Type.systemBars())
            onDispose { insets?.show(WindowInsetsCompat.Type.systemBars()) }
        }
        Box(Modifier.fillMaxSize().background(Color.Black)
            .clickable(role = Role.Button, onClickLabel = "Mostrar u ocultar controles") { controls = !controls }) {
            MonitorVideo(frames, Modifier.fillMaxSize().wrapContentSize(Alignment.Center), onNeedKeyframe)
            AnimatedVisibility(controls, Modifier.align(Alignment.TopEnd).windowInsetsPadding(WindowInsets.safeDrawing).padding(TyuDimens.Page),
                enter = fadeIn(tween(TyuMotion.State)), exit = fadeOut(tween(TyuMotion.Press))) {
                TyuIconButton(TyuIcons.Close, "Volver al dispositivo", onBack)
            }
            AnimatedVisibility(controls, Modifier.align(Alignment.BottomCenter).windowInsetsPadding(WindowInsets.safeDrawing).padding(TyuDimens.Page),
                enter = fadeIn(tween(TyuMotion.State)), exit = fadeOut(tween(TyuMotion.Press))) {
                TyuButton("Detener monitor", onStop, icon = TyuIcons.Stop, primary = false)
            }
        }
        return
    }
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
