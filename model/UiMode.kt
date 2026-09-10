package com.tyu.app.model

enum class UiMode(val title: String, val direction: String) {
    Monitor("Monitor", "PC → teléfono"),
    Mirror("Espejo", "teléfono → PC"),
    Bypass("Bypass", "teléfono ↔ PC"),
}
