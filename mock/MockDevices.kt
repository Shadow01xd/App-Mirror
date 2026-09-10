package com.tyu.app.mock

import com.tyu.app.model.UiDevice
import com.tyu.app.model.UiDeviceStatus

object MockDevices {
    val primary = UiDevice("rimi", "Dispositivo X", "Rimi-PC")
    val nearby = listOf(
        primary,
        UiDevice("pc-x", "PC X", "PC X", "192.168.1.108", UiDeviceStatus.Available),
        UiDevice("studio", "Studio", "Studio-PC", "192.168.1.112", UiDeviceStatus.Available),
    )
    fun byId(id: String) = nearby.firstOrNull { it.id == id } ?: primary
}
