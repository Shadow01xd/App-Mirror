package com.tyu.app.mock

import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.listSaver
import com.tyu.app.model.UiConnectionState
import com.tyu.app.model.UiMode
import com.tyu.app.model.UiSession

// Only visual state. No repositories, platform services or hardware dependencies.
@Stable
class MockAppState {
    var deviceId by mutableStateOf("rimi")
    var connection by mutableStateOf(UiConnectionState.Disconnected)
    var mode by mutableStateOf(UiMode.Monitor)
    var monitorActive by mutableStateOf(false)
    var mirrorActive by mutableStateOf(false)
    var cameraActive by mutableStateOf(false)
    var microphoneActive by mutableStateOf(false)
    var storageEnabled by mutableStateOf(true)
    var transferActive by mutableStateOf(false)
    var transferFinished by mutableStateOf(false)
    val device get() = MockDevices.byId(deviceId)
    val sessions get() = listOf(
        UiSession("Monitor", "PC → teléfono", monitorActive),
        UiSession("Espejo", "teléfono → PC", mirrorActive),
        UiSession("Bypass", "Servicios · sin pantalla", mode == UiMode.Bypass && connection == UiConnectionState.Connected),
        UiSession("Cámara", "Cámara TYU", cameraActive),
        UiSession("Micrófono", "Micrófono TYU", microphoneActive),
        UiSession("Almacenamiento", "Disponible", storageEnabled && connection == UiConnectionState.Connected),
        UiSession("Transferencia", "Enviando archivos", transferActive),
    )
    fun selectMode(value: UiMode) {
        mode = value
        if (value != UiMode.Monitor) monitorActive = false
        if (value != UiMode.Mirror) mirrorActive = false
    }
    fun disconnect() {
        connection = UiConnectionState.Disconnected
        monitorActive = false
        mirrorActive = false
        cameraActive = false
        microphoneActive = false
        transferActive = false
        transferFinished = false
    }
    companion object {
        val Saver = listSaver<MockAppState, Any>(
            save = { listOf(it.deviceId, it.connection.name, it.mode.name, it.monitorActive,
                it.mirrorActive, it.cameraActive, it.microphoneActive, it.storageEnabled,
                it.transferActive, it.transferFinished) },
            restore = { values -> MockAppState().apply {
                deviceId = values[0] as String
                connection = UiConnectionState.valueOf(values[1] as String)
                mode = UiMode.valueOf(values[2] as String)
                monitorActive = values[3] as Boolean
                mirrorActive = values[4] as Boolean
                cameraActive = values[5] as Boolean
                microphoneActive = values[6] as Boolean
                storageEnabled = values[7] as Boolean
                transferActive = values[8] as Boolean
                transferFinished = values[9] as Boolean
            } },
        )
    }
}
