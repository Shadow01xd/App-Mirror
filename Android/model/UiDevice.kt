package com.tyu.app.model

data class UiDevice(
    val id: String,
    val name: String,
    val computerName: String,
    val ip: String = "192.168.1.157",
    val status: UiDeviceStatus = UiDeviceStatus.Nearby,
    val connectionQuality: String = "Excelente",
    val nearby: Boolean = true,
)
