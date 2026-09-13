package com.tyu.app

import androidx.lifecycle.ViewModelProvider
import com.tyu.app.backend.ConnectionViewModel
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test

/** Requires a live Desktop invitation supplied as -e tyuPairingUri. No mock devices. */
class NativeConnectionTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private fun waitFor(text: String) {
        compose.waitUntil(90_000) { compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty() }
        compose.waitForIdle()
    }
    private fun click(text: String) {
        waitFor(text)
        val node = compose.onNodeWithText(text)
        runCatching { node.performScrollTo() }
        node.performClick()
    }
    @Test fun pair_approve_disconnect_and_trusted_reconnect() {
        val arguments = InstrumentationRegistry.getArguments()
        val restoreOnly = arguments.getString("tyuReconnectOnly") == "true"
        val nearby = arguments.getString("tyuNearby") == "true"
        val name = arguments.getString("tyuPeerName") ?: "TYU Desktop"
        click("START")
        click("Continuar")
        if (restoreOnly || nearby) {
            // On a real LAN this is the actual mDNS result, with no QR/trust prerequisite.
            click("Conectar")
        } else {
            val uri = requireNotNull(arguments.getString("tyuPairingUri")) {
                "Supply a live Desktop pairing URI with -e tyuPairingUri; approve the request in Desktop."
            }
            click("Conectar mediante QR")
            compose.onNodeWithText("Enlace TYU").assertDoesNotExist()
            compose.onNodeWithText("Escanear QR").assertExists()
            // The scanner calls this same ViewModel entry point. Supply a decoded
            // payload here; physical camera decoding is a separate acceptance check.
            compose.activityRule.scenario.onActivity { activity ->
                ViewModelProvider(activity)[ConnectionViewModel::class.java].pair(uri)
            }
        }
        waitFor(name) // Appears in the connected header only after native Connected.
        compose.onAllNodesWithText(name).onLast().performClick()
        click("Desconectar")
        waitFor("Tu próximo enlace")
        click("Conectar")
        waitFor(name)
        compose.activityRule.scenario.recreate()
        waitFor(name) // ViewModel retains the native connection across Activity recreation.
        compose.onAllNodesWithText(name).onLast().performClick()
        click("Desconectar")
        waitFor("Tu próximo enlace")
    }
}
