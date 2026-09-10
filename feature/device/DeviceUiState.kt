package com.tyu.app.feature.device

import com.tyu.app.model.UiDevice

data class DeviceUiState(val device: UiDevice, val cameraAllowed: Boolean, val microphoneAllowed: Boolean,
    val storageAllowed: Boolean)
