package com.tyu.app.preview

import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import com.tyu.app.feature.onboarding.*
import com.tyu.app.feature.permissions.*
import com.tyu.app.feature.nearby.*
import com.tyu.app.feature.pairing.*
import com.tyu.app.feature.home.*
import com.tyu.app.feature.monitor.*
import com.tyu.app.feature.mirror.*
import com.tyu.app.feature.bypass.*
import com.tyu.app.model.UiMode
import com.tyu.app.ui.components.*
import com.tyu.app.ui.theme.*

@Preview(name = "01 · Welcome", widthDp = 393, heightDp = 852)
@Preview(name = "Welcome · compacto", widthDp = 320, heightDp = 568)
@Composable
fun WelcomePreview() { TyuTheme { OnboardingScreen({}) } }

@Preview(name = "02 · Permisos", widthDp = 393, heightDp = 852)
@Composable
fun PermissionsPreview() { TyuTheme { PermissionsScreen({}, {}) } }

@Preview(name = "03 · Sin dispositivos", widthDp = 393, heightDp = 852)
@Composable
fun EmptyPreview() { TyuTheme { NearbyScreen(NearbyUiState.Empty, {}, {}, {}, {}, {}) } }

@Preview(name = "Buscando", widthDp = 393, heightDp = 852)
@Composable
fun SearchingPreview() { TyuTheme { NearbyScreen(NearbyUiState.Searching, {}, {}, {}, {}, {}) } }

@Preview(name = "04 · Dispositivos encontrados", widthDp = 393, heightDp = 852)
@Composable
fun FoundPreview() { TyuTheme { NearbyScreen(NearbyUiState.Found, {}, {}, {}, {}, {}) } }

@Preview(name = "05 · Error de conexión", widthDp = 393, heightDp = 852)
@Composable
fun ErrorPreview() {
    TyuTheme { PairingScreen(PairingUiState.Error, PreviewDevices.connected, {}, {}) }
}

@Preview(name = "06 · Conectado", widthDp = 393, heightDp = 852)
@Preview(name = "Conectado · fuente grande", widthDp = 393, heightDp = 852, fontScale = 1.5f)
@Preview(name = "Conectado · horizontal", widthDp = 852, heightDp = 393)
@Composable
fun ConnectedPreview() {
    TyuTheme { HomeScreen(HomeUiState(PreviewDevices.connected), {}, {}, {}, {}, {}) }
}

@Preview(name = "Monitor", widthDp = 393, heightDp = 852)
@Composable
fun MonitorPreview() { TyuTheme { MonitorScreen("Rimi-PC", {}, {}) } }

@Preview(name = "Espejo", widthDp = 393, heightDp = 852)
@Composable
fun MirrorPreview() { TyuTheme { MirrorScreen("Rimi-PC", MirrorUiState.Idle, {}, {}) } }

@Preview(name = "Bypass", widthDp = 393, heightDp = 852)
@Composable
fun BypassPreview() { TyuTheme { BypassScreen("Rimi-PC", false, false, true, {}, {}) } }

@Preview(name = "Sistema de componentes", widthDp = 393, heightDp = 852)
@Composable
fun ComponentsPreview() {
    TyuTheme {
        TyuPage {
            TyuButton("Acción principal", {}, Modifier.fillMaxWidth())
            TyuSecondaryButton("Acción secundaria", {}, Modifier.fillMaxWidth())
            TyuButton("No disponible", {}, Modifier.fillMaxWidth(), enabled = false)
            TyuToggle("Activado", "Estado seleccionado", true, {})
            TyuToggle("Desactivado", "Estado no seleccionado", false, {})
            TyuStatusDot("Conectado")
            TyuStatusDot("Desconectado", active = false)
            TyuStatusDot("Error", error = true)
            TyuProgressBar(.53f)
            TyuModeSelector(UiMode.Bypass, {})
        }
    }
}
