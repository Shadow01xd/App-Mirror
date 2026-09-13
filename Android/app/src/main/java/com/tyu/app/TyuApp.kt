package com.tyu.app

import androidx.compose.runtime.Composable
import com.tyu.app.navigation.TyuNavGraph
import com.tyu.app.ui.theme.TyuTheme

@Composable
fun TyuApp(fixture: Boolean = false, pairingUri: String? = null) {
    TyuTheme { TyuNavGraph(fixture, pairingUri) }
}
