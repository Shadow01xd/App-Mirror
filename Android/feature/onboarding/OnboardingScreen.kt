package com.tyu.app.feature.onboarding

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.dp
import com.tyu.app.ui.components.*
import com.tyu.app.ui.icons.*
import com.tyu.app.ui.theme.*

@Composable
fun OnboardingScreen(onStart: () -> Unit, state: OnboardingUiState = OnboardingUiState()) {
    val compact = LocalConfiguration.current.screenHeightDp < 500
    TyuPage(showBrand = false, spread = true, bottom = {
        TyuButton("START", onStart, Modifier.fillMaxWidth().padding(vertical = TyuDimens.Medium), TyuIcons.Arrow)
    }) {
        Column(Modifier.fillMaxWidth().padding(top = TyuDimens.Page), verticalArrangement = Arrangement.spacedBy(TyuDimens.Large)) {
            Text(state.title, color = TyuColors.Primary, style = MaterialTheme.typography.displayLarge)
            Text("Tu teléfono.\nTu PC.\nUn mismo espacio.", style = MaterialTheme.typography.headlineLarge)
            Text(state.subtitle, color = TyuColors.Secondary, style = MaterialTheme.typography.bodyLarge)
        }
        TyuDeviceStage(Modifier.fillMaxWidth().padding(vertical = TyuDimens.Page).height(if (compact) 140.dp else 210.dp))
    }
}
