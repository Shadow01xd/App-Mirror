package com.tyu.app.feature.home

import com.tyu.app.model.UiDevice
import com.tyu.app.model.UiMode

data class HomeUiState(val device: UiDevice, val mode: UiMode = UiMode.Monitor, val activeCount: Int = 0)
