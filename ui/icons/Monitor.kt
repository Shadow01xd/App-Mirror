package com.tyu.app.ui.icons

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.DesktopWindows
import androidx.compose.material.icons.outlined.PhoneAndroid
import androidx.compose.material.icons.outlined.SwapHoriz
import com.tyu.app.model.UiMode

fun UiMode.icon() = when (this) {
    UiMode.Monitor -> Icons.Outlined.DesktopWindows
    UiMode.Mirror -> Icons.Outlined.PhoneAndroid
    UiMode.Bypass -> Icons.Outlined.SwapHoriz
}
