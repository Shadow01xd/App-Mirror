package com.tyu.app.mock

import com.tyu.app.model.UiTransfer

object MockTransfers {
    val queue = listOf(
        UiTransfer("foto.jpg", "3.2 MB", 1f),
        UiTransfer("video.mp4", "48.6 MB", .53f),
        UiTransfer("documento.pdf", "1.4 MB", null),
    )
}
