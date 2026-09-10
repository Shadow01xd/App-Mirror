package com.tyu.app.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Shapes
import androidx.compose.ui.unit.dp

object TyuShapes {
    val Card = RoundedCornerShape(16.dp)
    val Control = RoundedCornerShape(12.dp)
    val Pill = RoundedCornerShape(50)
}
val TyuMaterialShapes = Shapes(
    small = TyuShapes.Control,
    medium = TyuShapes.Card,
    large = TyuShapes.Card,
)
