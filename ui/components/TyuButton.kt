package com.tyu.app.ui.components

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import com.tyu.app.ui.theme.*

@Composable
fun TyuButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier,
    icon: ImageVector? = null, primary: Boolean = true, enabled: Boolean = true) {
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(if (pressed) 0.98f else 1f,
        tween(TyuMotion.Press, easing = TyuMotion.Ease), label = "Presión del botón")
    Button(onClick = onClick,
        modifier = modifier.heightIn(min = TyuDimens.ButtonHeight).graphicsLayer { scaleX = scale; scaleY = scale },
        interactionSource = interaction, enabled = enabled, shape = TyuShapes.Control,
        border = if (!primary && enabled) BorderStroke(TyuDimens.Tiny / 4, TyuColors.Border) else null,
        contentPadding = PaddingValues(horizontal = TyuDimens.Page, vertical = TyuDimens.Gap),
        colors = ButtonDefaults.buttonColors(
            containerColor = if (primary) TyuColors.Primary else TyuColors.Surface,
            contentColor = if (primary) TyuColors.OnPrimary else TyuColors.Text,
            disabledContainerColor = TyuColors.Surface, disabledContentColor = TyuColors.Muted)) {
        Text(text, style = MaterialTheme.typography.labelLarge)
        if (icon != null) {
            Spacer(Modifier.width(TyuDimens.Medium))
            Icon(icon, null, Modifier.size(TyuDimens.Icon))
        }
    }
}

@Composable
fun TyuPrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier,
    icon: ImageVector? = null, enabled: Boolean = true) = TyuButton(text, onClick, modifier, icon, true, enabled)

@Composable
fun TyuSecondaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier,
    icon: ImageVector? = null, enabled: Boolean = true) = TyuButton(text, onClick, modifier, icon, false, enabled)
