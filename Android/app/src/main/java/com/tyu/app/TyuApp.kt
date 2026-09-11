package com.tyu.app

import androidx.compose.runtime.Composable
import com.tyu.app.navigation.TyuNavGraph
import com.tyu.app.ui.theme.TyuTheme

@Composable
fun TyuApp() {
    TyuTheme { TyuNavGraph() }
}
