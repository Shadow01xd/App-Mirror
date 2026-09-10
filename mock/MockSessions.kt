package com.tyu.app.mock

import com.tyu.app.model.UiSession

object MockSessions {
    val example = listOf(
        UiSession("Monitor", "Escritorio extendido", true),
        UiSession("Cámara", "Principal · 1080p", true),
        UiSession("Micrófono", "Reducción de ruido", true),
        UiSession("Almacenamiento", "Disponible", true),
    )
}
