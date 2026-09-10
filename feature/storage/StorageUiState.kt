package com.tyu.app.feature.storage

data class StorageUiState(val enabled: Boolean = true) {
    companion object {
        val categories = listOf("Imágenes", "Videos", "Música", "Documentos", "Descargas")
    }
}
