package com.tyu.app.feature.settings

data class SettingsUiState(
    val nearby: Boolean = true,
    val wifi: Boolean = true,
    val bluetooth: Boolean = false,
    val wifiDirect: Boolean = false,
    val confirmFiles: Boolean = true,
)
