package com.tyu.app.navigation

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
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
import com.tyu.app.feature.transfer.*
import com.tyu.app.feature.device.*
import com.tyu.app.feature.sessions.*
import com.tyu.app.feature.settings.*
import com.tyu.app.mock.MockAppState
import com.tyu.app.model.*
import kotlinx.coroutines.delay

@Composable
fun TyuNavGraph() {
    val app = rememberSaveable(saver = MockAppState.Saver) { MockAppState() }
    var route by rememberSaveable { mutableStateOf(TyuRoute.Welcome) }
    var history by rememberSaveable { mutableStateOf(listOf<String>()) }
    var nearby by rememberSaveable { mutableStateOf(NearbyUiState.Empty) }
    var failAttempt by rememberSaveable { mutableStateOf(false) }
    var mirror by rememberSaveable { mutableStateOf(MirrorUiState.Idle) }
    val screenStates = rememberSaveableStateHolder()

    fun navigate(next: TyuRoute) {
        if (next != route) {
            history = ArrayList(history + route.name)
            route = next
        }
    }
    fun back() {
        if (route == TyuRoute.Connecting) app.connection = UiConnectionState.Found
        mirror = MirrorUiState.Idle
        if (history.isNotEmpty()) {
            route = TyuRoute.valueOf(history.last())
            history = ArrayList(history.dropLast(1))
        } else route = if (app.connection == UiConnectionState.Connected) TyuRoute.Home else TyuRoute.Welcome
    }
    fun modeRoute(mode: UiMode): TyuRoute = when (mode) {
        UiMode.Monitor -> if (app.monitorActive) TyuRoute.MonitorActive else TyuRoute.Monitor
        UiMode.Mirror -> if (app.mirrorActive) TyuRoute.MirrorActive else TyuRoute.Mirror
        UiMode.Bypass -> TyuRoute.Bypass
    }
    fun openFeature(feature: UiFeature) {
        navigate(when (feature) {
            UiFeature.Storage -> TyuRoute.Storage
            UiFeature.Camera -> if (app.cameraActive) TyuRoute.CameraActive else TyuRoute.Camera
            UiFeature.Microphone -> if (app.microphoneActive) TyuRoute.MicrophoneActive else TyuRoute.Microphone
            UiFeature.Transfer -> TyuRoute.Transfer
        })
    }
    fun connect() {
        app.connection = UiConnectionState.Connecting
        route = TyuRoute.Connecting
    }

    BackHandler(history.isNotEmpty() || route != TyuRoute.Welcome && route != TyuRoute.Home) { back() }

    // Timed transitions are only presentation states and are cancelled when leaving the screen.
    LaunchedEffect(route, nearby) {
        if (route == TyuRoute.Nearby && nearby == NearbyUiState.Searching) {
            delay(1400)
            nearby = NearbyUiState.Found
            app.connection = UiConnectionState.Found
        }
        if (route == TyuRoute.Connecting) {
            delay(1200)
            if (failAttempt) {
                app.connection = UiConnectionState.Error
                route = TyuRoute.ConnectionError
            } else {
                app.connection = UiConnectionState.Connected
                history = arrayListOf()
                route = TyuRoute.Home
            }
        }
    }
    LaunchedEffect(route, mirror) {
        if (route == TyuRoute.Mirror && mirror == MirrorUiState.Connecting) {
            delay(1000)
            app.mirrorActive = true
            mirror = MirrorUiState.Active
            route = TyuRoute.MirrorActive
        }
    }

    AnimatedContent(targetState = route, transitionSpec = { tyuNavigationTransition() }, label = "Pantalla TYU") { current ->
        screenStates.SaveableStateProvider(current.name) {
            val computer = app.device.computerName
            when (current) {
                TyuRoute.Welcome -> OnboardingScreen({ navigate(TyuRoute.Permissions) })
                TyuRoute.Permissions -> PermissionsScreen(
                    onContinue = {
                        if (history.lastOrNull() == TyuRoute.Settings.name) back()
                        else navigate(TyuRoute.Nearby)
                    }, onBack = ::back)
                TyuRoute.Nearby -> NearbyScreen(nearby,
                    onSearch = { nearby = NearbyUiState.Searching; app.connection = UiConnectionState.Searching },
                    onPair = { device ->
                        app.deviceId = device.id
                        failAttempt = device.id == "pc-x"
                        history = ArrayList(history + TyuRoute.Nearby.name)
                        connect()
                    },
                    onQr = { navigate(TyuRoute.Qr) }, onBack = ::back,
                    onSettings = { navigate(TyuRoute.Settings) })
                TyuRoute.Qr, TyuRoute.Connecting, TyuRoute.ConnectionError -> PairingScreen(
                    state = when (current) {
                        TyuRoute.Qr -> PairingUiState.Qr
                        TyuRoute.Connecting -> PairingUiState.Connecting
                        else -> PairingUiState.Error
                    }, device = app.device,
                    onContinue = {
                        if (current == TyuRoute.Qr) app.deviceId = "rimi"
                        failAttempt = false
                        connect()
                    }, onCancel = ::back)
                TyuRoute.Home -> HomeScreen(HomeUiState(app.device, app.mode, app.sessions.count { it.active }),
                    onMode = { app.selectMode(it); navigate(modeRoute(it)) },
                    onOpenMode = { navigate(modeRoute(app.mode)) },
                    onDetails = { navigate(TyuRoute.Device) },
                    onSettings = { navigate(TyuRoute.Settings) },
                    onSessions = { navigate(TyuRoute.Sessions) })
                TyuRoute.Monitor -> MonitorScreen(computer,
                    onStart = { app.selectMode(UiMode.Monitor); app.monitorActive = true; route = TyuRoute.MonitorActive },
                    onBack = ::back)
                TyuRoute.MonitorActive -> MonitorActiveScreen(computer,
                    onStop = { app.monitorActive = false; route = TyuRoute.Monitor }, onBack = ::back)
                TyuRoute.Mirror -> MirrorScreen(computer, if (mirror == MirrorUiState.Active) MirrorUiState.Idle else mirror,
                    onStart = { app.selectMode(UiMode.Mirror); mirror = MirrorUiState.Connecting }, onBack = ::back)
                TyuRoute.MirrorActive -> MirrorActiveScreen(computer,
                    onStop = { app.mirrorActive = false; mirror = MirrorUiState.Idle; route = TyuRoute.Mirror },
                    onBack = ::back)
                TyuRoute.Bypass -> BypassScreen(computer, app.cameraActive, app.microphoneActive,
                    app.storageEnabled, ::openFeature, ::back)
                TyuRoute.Camera -> CameraScreen(computer,
                    onStart = { app.cameraActive = true; route = TyuRoute.CameraActive }, onBack = ::back)
                TyuRoute.CameraActive -> CameraActiveScreen(computer,
                    onStop = { app.cameraActive = false; route = TyuRoute.Camera }, onBack = ::back)
                TyuRoute.Microphone -> MicrophoneScreen(computer,
                    onStart = { app.microphoneActive = true; route = TyuRoute.MicrophoneActive }, onBack = ::back)
                TyuRoute.MicrophoneActive -> MicrophoneActiveScreen(computer,
                    onStop = { app.microphoneActive = false; route = TyuRoute.Microphone }, onBack = ::back)
                TyuRoute.Storage -> StorageScreen(computer, StorageUiState(app.storageEnabled),
                    onEnabled = { app.storageEnabled = it }, onBack = ::back)
                TyuRoute.Transfer -> TransferScreen(computer,
                    onSend = { app.transferActive = true; app.transferFinished = false; navigate(TyuRoute.TransferProgress) },
                    onQueue = { navigate(TyuRoute.TransferProgress) },
                    hasQueue = app.transferActive || app.transferFinished, onBack = ::back)
                TyuRoute.TransferProgress -> TransferProgressScreen(computer,
                    TransferUiState(app.transferActive, app.transferFinished),
                    onComplete = { app.transferActive = false; app.transferFinished = true },
                    onCancel = { app.transferActive = false }, onBack = ::back)
                TyuRoute.Device -> DeviceScreen(
                    DeviceUiState(app.device, app.cameraActive, app.microphoneActive, app.storageEnabled),
                    onForget = {
                        app.disconnect()
                        nearby = NearbyUiState.Empty
                        history = arrayListOf()
                        route = TyuRoute.Nearby
                    }, onStorage = { app.storageEnabled = it }, onCamera = { app.cameraActive = it },
                    onMicrophone = { app.microphoneActive = it }, onBack = ::back)
                TyuRoute.Sessions -> SessionsScreen(SessionsUiState(app.sessions),
                    onOpen = { title ->
                        when (title) {
                            "Monitor" -> { app.selectMode(UiMode.Monitor); navigate(modeRoute(UiMode.Monitor)) }
                            "Espejo" -> { app.selectMode(UiMode.Mirror); navigate(modeRoute(UiMode.Mirror)) }
                            "Bypass" -> { app.selectMode(UiMode.Bypass); navigate(TyuRoute.Bypass) }
                            "Cámara" -> openFeature(UiFeature.Camera)
                            "Micrófono" -> openFeature(UiFeature.Microphone)
                            "Almacenamiento" -> openFeature(UiFeature.Storage)
                            "Transferencia" -> navigate(if (app.transferActive) TyuRoute.TransferProgress else TyuRoute.Transfer)
                        }
                    }, onBack = ::back)
                TyuRoute.Settings -> SettingsScreen(app.connection == UiConnectionState.Connected,
                    onPermissions = { navigate(TyuRoute.Permissions) },
                    onDevice = { navigate(TyuRoute.Device) }, onBack = ::back)
            }
        }
    }
}
