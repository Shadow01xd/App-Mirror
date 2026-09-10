package com.tyu.app.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

/** Optional local brand font is retained for metadata; UI uses native Android sans. */
@Composable
fun tyuFontFamily(): FontFamily {
    val context = LocalContext.current
    return remember(context) {
        val id = context.resources.getIdentifier("iosevka_charon", "font", context.packageName)
        if (id != 0) FontFamily(Font(id)) else FontFamily.Monospace
    }
}

fun TyuTypography(font: FontFamily = FontFamily.Monospace): Typography {
    val sans = FontFamily.SansSerif
    return Typography(
        displayLarge = TextStyle(fontFamily = sans, fontWeight = FontWeight.Bold, fontSize = 76.sp, lineHeight = 80.sp, letterSpacing = (-3).sp),
        displayMedium = TextStyle(fontFamily = sans, fontWeight = FontWeight.Medium, fontSize = 44.sp, lineHeight = 48.sp, letterSpacing = (-1.5).sp),
        displaySmall = TextStyle(fontFamily = sans, fontWeight = FontWeight.Bold, fontSize = 28.sp, lineHeight = 34.sp, letterSpacing = (-1).sp),
        headlineLarge = TextStyle(fontFamily = sans, fontWeight = FontWeight.Medium, fontSize = 36.sp, lineHeight = 41.sp, letterSpacing = (-1).sp),
        headlineMedium = TextStyle(fontFamily = sans, fontWeight = FontWeight.Medium, fontSize = 30.sp, lineHeight = 36.sp, letterSpacing = (-0.7).sp),
        headlineSmall = TextStyle(fontFamily = sans, fontWeight = FontWeight.Medium, fontSize = 24.sp, lineHeight = 30.sp),
        titleLarge = TextStyle(fontFamily = sans, fontWeight = FontWeight.Medium, fontSize = 22.sp, lineHeight = 28.sp),
        titleMedium = TextStyle(fontFamily = sans, fontWeight = FontWeight.Medium, fontSize = 17.sp, lineHeight = 24.sp),
        titleSmall = TextStyle(fontFamily = sans, fontWeight = FontWeight.Medium, fontSize = 15.sp, lineHeight = 22.sp),
        bodyLarge = TextStyle(fontFamily = sans, fontSize = 16.sp, lineHeight = 24.sp),
        bodyMedium = TextStyle(fontFamily = sans, fontSize = 14.sp, lineHeight = 21.sp),
        bodySmall = TextStyle(fontFamily = sans, fontSize = 12.sp, lineHeight = 18.sp),
        labelLarge = TextStyle(fontFamily = sans, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, lineHeight = 21.sp),
        labelMedium = TextStyle(fontFamily = font, fontSize = 11.sp, lineHeight = 17.sp, letterSpacing = 0.5.sp),
        labelSmall = TextStyle(fontFamily = sans, fontWeight = FontWeight.Medium, fontSize = 12.sp, lineHeight = 17.sp),
    )
}
