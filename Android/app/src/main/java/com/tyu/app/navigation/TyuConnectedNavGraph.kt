package com.tyu.app.navigation

import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.compose.animation.AnimatedContent
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.viewmodel.compose.viewModel
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import com.tyu.app.backend.ConnectionViewModel
import com.tyu.app.feature.onboarding.*
import com.tyu.app.feature.permissions.*
import com.tyu.app.feature.nearby.*
import com.tyu.app.feature.pairing.*
import com.tyu.app.feature.home.*
import com.tyu.app.feature.monitor.*
import com.tyu.app.feature.mirror.*
import com.tyu.app.feature.bypass.*
import com.tyu.app.feature.camera.*
import com.tyu.app.feature.microphone.*
import com.tyu.app.feature.storage.*
import com.tyu.app.feature.device.*
import com.tyu.app.feature.sessions.*
import com.tyu.app.feature.settings.*
import com.tyu.app.model.*
import com.tyu.app.ui.components.TyuDialog

@Composable
fun TyuNavGraph(fixture: Boolean = false, pairingUri: String? = null) {
    if (fixture) TyuFixtureNavGraph() else TyuConnectedNavGraph(pairingUri)
}

/** Existing Compose surfaces, driven only by actual native connection events. */
@Composable
private fun TyuConnectedNavGraph(pairingUri: String?, backend: ConnectionViewModel = viewModel()) {
    val state by backend.state.collectAsState()
    val context = LocalContext.current
    var route by rememberSaveable { mutableStateOf(TyuRoute.Welcome) }
    var history by rememberSaveable { mutableStateOf(listOf<TyuRoute>()) }
    var mode by rememberSaveable { mutableStateOf(UiMode.Monitor) }
    var invitation by rememberSaveable { mutableStateOf("") }
    var selectedId by rememberSaveable { mutableStateOf<String?>(null) }
    var showError by remember { mutableStateOf(false) }
    var hadConnection by rememberSaveable { mutableStateOf(false) }
    fun navigate(next: TyuRoute) { if (route != next) { history = history + route; route = next } }
    fun back() {
        if (state.connectingId != null) backend.disconnect()
        route = history.lastOrNull() ?: if (state.connected != null) TyuRoute.Home else TyuRoute.Nearby
        history = history.dropLast(1)
    }
    fun unavailable() { Toast.makeText(context, "Esta función todavía no tiene integración de plataforma", Toast.LENGTH_LONG).show() }
    fun modeRoute() = when (mode) { UiMode.Monitor -> TyuRoute.Monitor; UiMode.Mirror -> TyuRoute.Mirror; UiMode.Bypass -> TyuRoute.Bypass }
    val scanner = rememberLauncherForActivityResult(ScanContract()) { result ->
        result.contents?.let { invitation = it; if (it.startsWith("tyu://pair/")) backend.pair(it) else showError = true }
    }
    LaunchedEffect(pairingUri, state.ready) {
        if (state.ready && pairingUri != null && invitation != pairingUri) {
            invitation = pairingUri
            backend.pair(pairingUri)
        }
    }
    LaunchedEffect(state.connected?.id, state.connectingId) {
        if (state.connected != null) { route = TyuRoute.Home; history = emptyList(); hadConnection = true }
        else if (state.connectingId != null) { route = TyuRoute.Connecting }
        else if (route == TyuRoute.Connecting || hadConnection) { route = TyuRoute.Nearby; hadConnection = false }
    }
    LaunchedEffect(state.error) { if (state.error != null) showError = true }
    BackHandler(route != TyuRoute.Welcome && route != TyuRoute.Home) { back() }
    val device = state.connected?.ui(true)
        ?: state.peers.find { it.id == selectedId }?.ui()
        ?: UiDevice("", "Tu equipo", "tu PC", ip = "", connectionQuality = "Sin conexión", nearby = false)
    AnimatedContent(targetState = route, transitionSpec = { tyuNavigationTransition(true) }, label = "Pantalla TYU") { current ->
        when (current) {
            TyuRoute.Welcome -> OnboardingScreen({ navigate(TyuRoute.Permissions) })
            TyuRoute.Permissions -> PermissionsScreen(onContinue = { navigate(TyuRoute.Nearby) }, onBack = ::back)
            TyuRoute.Nearby -> NearbyScreen(
                state = if (state.peers.isNotEmpty()) NearbyUiState.Found else if (state.scanning) NearbyUiState.Searching else NearbyUiState.Empty,
                onSearch = backend::discover,
                onPair = { ui -> selectedId = ui.id; state.peers.find { it.id == ui.id }?.let(backend::connect) },
                onQr = { navigate(TyuRoute.Qr) }, onBack = ::back,
                onSettings = { navigate(TyuRoute.Settings) }, devices = state.peers.map { it.ui() })
            TyuRoute.Qr, TyuRoute.Connecting, TyuRoute.ConnectionError -> PairingScreen(
                state = if (current == TyuRoute.Connecting) PairingUiState.Connecting else if (current == TyuRoute.ConnectionError) PairingUiState.Error else PairingUiState.Qr,
                device = device, onContinue = { backend.pair(invitation) }, onCancel = ::back,
                native = true, error = state.error,
                onScan = { scanner.launch(ScanOptions().setDesiredBarcodeFormats(ScanOptions.QR_CODE).setPrompt("Escanea el QR de TYU Desktop").setBeepEnabled(false).setOrientationLocked(false)) })
            TyuRoute.Home -> HomeScreen(HomeUiState(device, mode, 0),
                onMode = { mode = it; navigate(modeRoute()) }, onOpenMode = { navigate(modeRoute()) },
                onDetails = { navigate(TyuRoute.Device) }, onSettings = { navigate(TyuRoute.Settings) },
                onSessions = { navigate(TyuRoute.Sessions) })
            TyuRoute.Monitor, TyuRoute.MonitorActive -> MonitorScreen(device.computerName, onStart = ::unavailable, onBack = ::back)
            TyuRoute.Mirror, TyuRoute.MirrorActive -> MirrorScreen(device.computerName, MirrorUiState.Idle, onStart = ::unavailable, onBack = ::back)
            TyuRoute.Bypass -> BypassScreen(device.computerName, false, false, false,
                onFeature = { feature -> when (feature) {
                    UiFeature.Camera -> navigate(TyuRoute.Camera)
                    UiFeature.Microphone -> navigate(TyuRoute.Microphone)
                    UiFeature.Storage -> navigate(TyuRoute.Storage)
                    UiFeature.Transfer -> unavailable()
                } }, onBack = ::back, status = "Conectado · Sin servicios activos")
            TyuRoute.Camera, TyuRoute.CameraActive -> CameraScreen(device.computerName, onStart = ::unavailable, onBack = ::back)
            TyuRoute.Microphone, TyuRoute.MicrophoneActive -> MicrophoneScreen(device.computerName, onStart = ::unavailable, onBack = ::back)
            TyuRoute.Storage -> StorageScreen(device.computerName, StorageUiState(false), onEnabled = { unavailable() }, onBack = ::back)
            TyuRoute.Device -> DeviceScreen(DeviceUiState(device, false, false, false),
                onForget = backend::forget, onStorage = { unavailable() }, onCamera = { unavailable() },
                onMicrophone = { unavailable() }, onBack = ::back, onDisconnect = backend::disconnect)
            TyuRoute.Sessions -> SessionsScreen(SessionsUiState(emptyList()), onOpen = {}, onBack = ::back)
            TyuRoute.Settings -> SettingsScreen(state.connected != null, onPermissions = { navigate(TyuRoute.Permissions) },
                onDevice = { if (state.connected != null) navigate(TyuRoute.Device)
                    else Toast.makeText(context, "Conecta un equipo para ver sus detalles", Toast.LENGTH_SHORT).show() }, onBack = ::back)
            else -> NearbyScreen(NearbyUiState.Empty, backend::discover, {}, { navigate(TyuRoute.Qr) }, ::back, { navigate(TyuRoute.Settings) }, emptyList())
        }
    }
    if (showError) TyuDialog("Conexión TYU", state.error ?: "El código no es una invitación TYU válida", "Aceptar",
        { showError = false; backend.clearError() }, { showError = false; backend.clearError() })
}
