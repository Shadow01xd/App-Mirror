package com.tyu.app.feature.permissions

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import com.tyu.app.ui.components.*
import com.tyu.app.ui.icons.*
import com.tyu.app.ui.theme.*

@Composable
fun PermissionsScreen(onContinue: () -> Unit, onBack: () -> Unit,
    initial: PermissionsUiState = PermissionsUiState()) {
    var nearby by rememberSaveable { mutableStateOf(initial.nearby) }
    var network by rememberSaveable { mutableStateOf(initial.localNetwork) }
    TyuPage(onBack = onBack, spread = true) {
        TyuHeading("Un poco de confianza", "Necesitamos algunos permisos para que TYU funcione correctamente.")
        Column(verticalArrangement = Arrangement.spacedBy(TyuDimens.Gap),
            modifier = Modifier.padding(vertical = TyuDimens.Page)) {
            Box(Modifier.fillMaxWidth().tyuEnter(0)) {
                TyuPermissionCard("Dispositivos cercanos", "Encontrar tu PC a tu alrededor.",
                    TyuIcons.Devices, nearby, { nearby = it })
            }
            Box(Modifier.fillMaxWidth().tyuEnter(1)) {
                TyuPermissionCard("Red local", "Conectar tus dispositivos entre sí.",
                    TyuIcons.Wifi, network, { network = it })
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(TyuDimens.Page)) {
            TyuFootnote("Esta vista es una demostración. Los interruptores no solicitan permisos ni activan conexiones.")
            TyuButton("Continuar", onContinue, Modifier.fillMaxWidth(), TyuIcons.Arrow)
        }
    }
}
