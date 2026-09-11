package com.tyu.app.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable

private val Colors = darkColorScheme(
    primary = TyuColors.Primary,
    onPrimary = TyuColors.OnPrimary,
    secondary = TyuColors.Secondary,
    background = TyuColors.Background,
    onBackground = TyuColors.Text,
    surface = TyuColors.Surface,
    onSurface = TyuColors.Text,
    surfaceVariant = TyuColors.Elevated,
    onSurfaceVariant = TyuColors.Secondary,
    outline = TyuColors.Border,
    error = TyuColors.Error,
)

@Composable
fun TyuTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = Colors, typography = TyuTypography(tyuFontFamily()),
        shapes = TyuMaterialShapes, content = content)
}
