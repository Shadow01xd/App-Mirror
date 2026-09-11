package com.tyu.app.ui.components

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.text.style.TextAlign
import com.tyu.app.ui.theme.*

@Composable
fun TyuPage(
    onBack: (() -> Unit)? = null,
    onSettings: (() -> Unit)? = null,
    onSessions: (() -> Unit)? = null,
    showBrand: Boolean = true,
    spread: Boolean = false,
    bottom: @Composable () -> Unit = {},
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(Modifier.fillMaxSize(), color = TyuColors.Background) {
        Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing),
            horizontalAlignment = Alignment.CenterHorizontally) {
            if (showBrand) {
                Box(Modifier.widthIn(max = TyuDimens.ContentWidth).fillMaxWidth()) {
                    TyuTopBar(onBack, onSettings, onSessions)
                }
            }
            BoxWithConstraints(Modifier.weight(1f).widthIn(max = TyuDimens.ContentWidth).fillMaxWidth()) {
                val availableHeight = maxHeight
                Column(
                    Modifier.fillMaxWidth().verticalScroll(rememberScrollState())
                        .heightIn(min = availableHeight).padding(TyuDimens.Page),
                    verticalArrangement = if (spread) TyuSpread else Arrangement.spacedBy(TyuDimens.Page),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    content = content,
                )
            }
            Box(Modifier.widthIn(max = TyuDimens.ContentWidth).fillMaxWidth()
                .padding(horizontal = TyuDimens.Page)) { bottom() }
        }
    }
}

/** Space-between on tall screens, with a real minimum gap when content must scroll. */
private val TyuSpread = object : Arrangement.Vertical {
    override val spacing: Dp = TyuDimens.Page
    override fun Density.arrange(totalSize: Int, sizes: IntArray, outPositions: IntArray) {
        val gap = if (sizes.size > 1) maxOf(spacing.roundToPx(), (totalSize - sizes.sum()) / (sizes.size - 1)) else 0
        var position = 0
        sizes.forEachIndexed { index, size -> outPositions[index] = position; position += size + gap }
    }
}

@Composable
fun TyuHeading(title: String, subtitle: String? = null) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(TyuDimens.Small)) {
        Text(title, style = MaterialTheme.typography.headlineMedium)
        if (subtitle != null) Text(subtitle, color = TyuColors.Secondary, style = MaterialTheme.typography.bodyLarge)
    }
}

@Composable
fun TyuHero(icon: ImageVector, title: String, subtitle: String,
    error: Boolean = false) {
    val compact = LocalConfiguration.current.screenHeightDp < 500
    Column(Modifier.fillMaxWidth().padding(vertical = TyuDimens.Small),
        verticalArrangement = Arrangement.spacedBy(TyuDimens.Page)) {
        TyuSignalStage(icon, Modifier.fillMaxWidth().height(if (compact) TyuDimens.HeroIcon else TyuDimens.HeroIcon * 1.4f), error)
        TyuHeading(title, subtitle)
    }
}

@Composable
fun TyuFootnote(text: String) {
    Text(text, Modifier.fillMaxWidth(), color = TyuColors.Secondary,
        style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
}
