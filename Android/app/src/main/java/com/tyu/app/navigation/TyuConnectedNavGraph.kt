package com.tyu.app.navigation

import android.app.Activity
import android.content.Intent
import android.media.projection.MediaProjectionManager
import android.provider.Settings
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.viewmodel.compose.viewModel
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import com.tyu.app.backend.ConnectionViewModel
import com.tyu.app.backend.MirrorCaptureService
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
    var monitorOrientation by rememberSaveable { mutableStateOf("Automática") }
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
    // Mirror: this phone is the only capture source, so whenever a Mirror session exists (started
    // here or by the PC) it asks for the screen-capture grant, registers the stream, then hands
    // grant + stream to the foreground service that encodes and pushes frames.
    var pendingProjection by remember { mutableStateOf<Pair<Int, Intent>?>(null) }
    var projectionRequested by remember { mutableStateOf(false) }
    var mirrorMediaRequested by remember { mutableStateOf(false) }
    val projectionManager = context.getSystemService(MediaProjectionManager::class.java)
    val projectionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        if (result.resultCode == Activity.RESULT_OK && result.data != null) {
            pendingProjection = result.resultCode to result.data!!
        } else {
            Toast.makeText(context, "Sin permiso de captura no se puede transmitir la pantalla", Toast.LENGTH_LONG).show()
            backend.stopMirror()
        }
    }
    LaunchedEffect(state.mirrorSession) {
        if (state.mirrorSession != null && pendingProjection == null && !projectionRequested) {
            projectionRequested = true
            projectionLauncher.launch(projectionManager.createScreenCaptureIntent())
        }
    }
    LaunchedEffect(state.mirrorSession, pendingProjection) {
        if (state.mirrorSession != null && pendingProjection != null && !mirrorMediaRequested) {
            mirrorMediaRequested = true
            val metrics = context.resources.displayMetrics
            backend.startMirrorMedia(metrics.widthPixels, metrics.heightPixels, 60, 8_000_000)
        }
    }
    LaunchedEffect(state.mirrorStream) {
        val stream = state.mirrorStream
        val session = state.mirrorSession
        val peer = state.connected?.id
        val projection = pendingProjection
        if (stream != null && session != null && peer != null && projection != null) {
            val metrics = context.resources.displayMetrics
            context.startForegroundService(Intent(context, MirrorCaptureService::class.java).apply {
                putExtra(MirrorCaptureService.EXTRA_RESULT_CODE, projection.first)
                putExtra(MirrorCaptureService.EXTRA_DATA, projection.second)
                putExtra(MirrorCaptureService.EXTRA_HANDLE, backend.nativeHandle)
                putExtra(MirrorCaptureService.EXTRA_PEER, peer)
                putExtra(MirrorCaptureService.EXTRA_SESSION, session)
                putExtra(MirrorCaptureService.EXTRA_STREAM, stream)
                putExtra(MirrorCaptureService.EXTRA_WIDTH, metrics.widthPixels)
                putExtra(MirrorCaptureService.EXTRA_HEIGHT, metrics.heightPixels)
                putExtra(MirrorCaptureService.EXTRA_FPS, 60)
                putExtra(MirrorCaptureService.EXTRA_BITRATE, 8_000_000)
                putExtra(MirrorCaptureService.EXTRA_DENSITY, metrics.densityDpi)
            })
        }
    }
    LaunchedEffect(state.mirrorSession) {
        if (state.mirrorSession == null) {
            context.stopService(Intent(context, MirrorCaptureService::class.java))
            pendingProjection = null
            projectionRequested = false
            mirrorMediaRequested = false
        }
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
    fun openAccessibility() {
        Toast.makeText(context, "Activa \"TYU control táctil\". En Android 13+ puede pedir \"Permitir ajustes restringidos\" en Información de la app.", Toast.LENGTH_LONG).show()
        runCatching { context.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }
    LaunchedEffect(state.accessibilityMissing) {
        if (state.accessibilityMissing) {
            openAccessibility()
            backend.accessibilityPrompted()
        }
    }
    LaunchedEffect(state.mirrorSession) {
        if (state.mirrorSession != null && (route == TyuRoute.Mirror || route == TyuRoute.Home)) navigate(TyuRoute.MirrorActive)
        else if (state.mirrorSession == null && route == TyuRoute.MirrorActive) back()
    }
    val mirrorUiState = when {
        state.mirrorSession != null -> MirrorUiState.Active
        state.mirrorPending -> MirrorUiState.Connecting
        else -> MirrorUiState.Idle
    }
    // Monitor: PC -> phone. Requesting it here only negotiates the session; Desktop is the only
    // side that can actually capture, so it starts publishing once ModeStarted reaches it.
    LaunchedEffect(state.monitorSession) {
        if (state.monitorSession != null && (route == TyuRoute.Monitor || route == TyuRoute.Home)) navigate(TyuRoute.MonitorActive)
        else if (state.monitorSession == null && route == TyuRoute.MonitorActive) back()
    }
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
                onSettings = { navigate(TyuRoute.Settings) }, devices = state.peers.map { it.ui() },
                onUsb = {
                    Toast.makeText(context, "Conecta el cable y activa \"Compartir conexión por USB\"; TYU te preguntará si quieres conectarte por USB.", Toast.LENGTH_LONG).show()
                    runCatching { context.startActivity(Intent("android.settings.TETHER_SETTINGS").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
                        .onFailure { runCatching { context.startActivity(Intent(Settings.ACTION_WIRELESS_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) } }
                })
            TyuRoute.Qr, TyuRoute.Connecting, TyuRoute.ConnectionError -> PairingScreen(
                state = if (current == TyuRoute.Connecting) PairingUiState.Connecting else if (current == TyuRoute.ConnectionError) PairingUiState.Error else PairingUiState.Qr,
                device = device, onContinue = { backend.pair(invitation) }, onCancel = ::back,
                native = true, error = state.error,
                onScan = { scanner.launch(ScanOptions().setDesiredBarcodeFormats(ScanOptions.QR_CODE).setPrompt("Escanea el QR de TYU Desktop").setBeepEnabled(false).setOrientationLocked(false)) })
            TyuRoute.Home -> HomeScreen(HomeUiState(device, mode, 0),
                onMode = { mode = it; navigate(modeRoute()) }, onOpenMode = { navigate(modeRoute()) },
                onDetails = { navigate(TyuRoute.Device) }, onSettings = { navigate(TyuRoute.Settings) },
                onSessions = { navigate(TyuRoute.Sessions) })
            TyuRoute.Monitor -> MonitorScreen(device.computerName, onStart = backend::startMonitor, onBack = ::back,
                initial = MonitorUiState(orientation = monitorOrientation), onOrientation = { monitorOrientation = it })
            TyuRoute.MonitorActive -> MonitorActiveScreen(device.computerName, onStop = backend::stopMonitor, onBack = ::back,
                frames = backend.monitorFrames, orientation = monitorOrientation, onNeedKeyframe = backend::requestMonitorKeyframe)
            TyuRoute.Mirror -> MirrorScreen(device.computerName, mirrorUiState, onStart = backend::startMirror, onBack = ::back,
                controlEnabled = state.touchControl, onEnableControl = ::openAccessibility)
            TyuRoute.MirrorActive -> MirrorActiveScreen(device.computerName, onStop = backend::stopMirror, onBack = ::back,
                status = when {
                    pendingProjection == null -> "Esperando permiso de captura de pantalla…"
                    state.mirrorStream == null -> "Preparando la transmisión…"
                    else -> "Transmitiendo a ${device.computerName}"
                }, controlEnabled = state.touchControl, onEnableControl = ::openAccessibility)
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
    state.usbOffer?.let { offer ->
        TyuDialog("Cable USB detectado", "${offer.name} está conectado por cable. ¿Te quieres conectar por medio de USB?",
            "Conectar", { selectedId = offer.id; backend.answerUsb(true) }, { backend.answerUsb(false) })
    }
}
