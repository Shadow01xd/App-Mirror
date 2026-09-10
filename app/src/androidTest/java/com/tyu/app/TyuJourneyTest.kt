package com.tyu.app

import android.content.pm.ActivityInfo
import android.graphics.Bitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.semantics.SemanticsActions
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test
import java.io.File

/**
 * Runs the actual activity, navigates the public UI, and captures rendered Android screens.
 * Test-only screenshots are written to the app's external test output directory.
 */
class TyuJourneyTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    private fun waitFor(text: String) {
        compose.waitUntil(10_000) { compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty() }
        compose.waitForIdle()
    }
    private fun click(text: String) {
        waitFor(text)
        val node = compose.onNodeWithText(text)
        runCatching { node.performScrollTo() }
        node.performClick()
        compose.waitForIdle()
    }
    private fun back() {
        compose.onNodeWithContentDescription("Volver").performClick()
        compose.waitForIdle()
    }
    private fun screenshot(name: String) {
        compose.waitForIdle()
        // Capture the beginning of the screen, even after selecting an option below the fold.
        val scrolling = compose.onAllNodes(hasScrollAction())
        if (scrolling.fetchSemanticsNodes().isNotEmpty()) {
            scrolling[0].performSemanticsAction(SemanticsActions.ScrollBy) { it(0f, -100_000f) }
            compose.waitForIdle()
        }
        // Compose idle does not include the Android window rotation animation.
        android.os.SystemClock.sleep(600)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val output = InstrumentationRegistry.getArguments().getString("screenshotsFolder") ?: "ui-validation"
        val folder = File(instrumentation.targetContext.getExternalFilesDir(null), output).apply { mkdirs() }
        val bitmap = instrumentation.uiAutomation.takeScreenshot()
        File(folder, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
    }
    private fun found() {
        click("START")
        click("Continuar")
        click("Agregar dispositivo")
        waitFor("Tu próximo enlace")
    }
    private fun connect(index: Int = 0) {
        found()
        val node = compose.onAllNodesWithText("Conectar")[index]
        node.performScrollTo().performClick()
        waitFor(if (index == 1) "No pudimos conectar" else "Dispositivo X")
    }

    @Test fun completeJourneyAndServiceStates() {
        screenshot("01-welcome")
        click("START")
        screenshot("02-permissions")
        click("Continuar")
        screenshot("03-empty")
        click("Agregar dispositivo")
        waitFor("Tu próximo enlace")
        screenshot("04-found")
        compose.onAllNodesWithText("Conectar")[0].performScrollTo().performClick()
        waitFor("Dispositivo X")
        screenshot("06-connected")

        click("Monitor")
        click("Horizontal")
        screenshot("07-monitor")
        click("Usar como monitor")
        waitFor("Monitor TYU activo")
        screenshot("08-monitor-active")
        click("Detener monitor")
        back()

        click("Espejo")
        screenshot("09-mirror")
        click("Transmitir pantalla")
        waitFor("Espejo activo")
        screenshot("10-mirror-active")
        click("Detener")
        back()

        click("Bypass")
        screenshot("11-bypass")
        click("Cámara")
        screenshot("12-camera")
        click("Activar cámara")
        waitFor("Cámara activa")
        screenshot("13-camera-active")
        back()
        click("Micrófono")
        click("Reducción de ruido")
        screenshot("14-microphone")
        click("Activar micrófono")
        waitFor("Micrófono activo")
        screenshot("15-microphone-active")
        back()
        click("Almacenamiento")
        screenshot("16-storage")
        click("Compartir almacenamiento")
        compose.onNodeWithText("Imágenes").assertIsNotEnabled()
        click("Compartir almacenamiento")
        back()

        click("Enviar archivos")
        screenshot("17-transfer")
        click("Videos")
        compose.onNodeWithText("Enviar archivos").assertIsNotEnabled()
        click("Elegir archivos")
        click("Seleccionar ejemplos")
        click("Enviar archivos")
        waitFor("Enviando a Rimi-PC")
        compose.onNodeWithText("53 %").assertExists()
        screenshot("18-transfer-progress")
        click("Completar vista previa")
        waitFor("Todo en su lugar")
        screenshot("19-transfer-complete")
        back()
        back()
        back()
        compose.onNodeWithContentDescription("Actividad actual").performClick()
        waitFor("Actividad actual")
        screenshot("20-sessions")
        compose.onNodeWithText("Cámara TYU").assertExists()
        compose.onNodeWithText("Micrófono TYU").assertExists()
        back()
        click("Rimi-PC")
        screenshot("21-device")
        compose.onNodeWithText("192.168.1.157").assertExists()
        back()
        compose.onNodeWithContentDescription("Configuración").performClick()
        waitFor("Configuración")
        screenshot("22-settings")
        click("Recibidos")
        compose.onNodeWithText("Todavía no hay archivos recibidos.").assertExists()
        click("Cerrar")
        click("Permisos")
        click("Continuar")
        waitFor("Configuración")
    }

    @Test fun failedPairingCanRetry() {
        connect(index = 1)
        screenshot("05-connection-error")
        click("Reintentar")
        waitFor("PC X")
        compose.onNodeWithText("Conectado").assertExists()
    }

    @Test fun qrCancelAndForgetReturnToEmptyState() {
        click("START")
        click("Continuar")
        click("Conectar mediante QR")
        screenshot("23-qr")
        click("Cancelar")
        waitFor("Tu PC, a un paso")
        click("Conectar mediante QR")
        click("Continuar")
        waitFor("Dispositivo X")
        click("Rimi-PC")
        click("Olvidar dispositivo")
        click("Cancelar")
        compose.onNodeWithText("192.168.1.157").assertExists()
        click("Olvidar dispositivo")
        click("Olvidar")
        waitFor("Tu PC, a un paso")
    }

    @Test fun recreationAndLandscapePreserveConnectionAndOptions() {
        connect()
        click("Monitor")
        click("Vertical")
        compose.activityRule.scenario.recreate()
        waitFor("Usar como monitor")
        compose.onNodeWithText("Vertical").assertIsSelected()
        compose.activityRule.scenario.onActivity {
            it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        }
        waitFor("Usar como monitor")
        // The redesigned primary action stays outside the scrolling options.
        compose.onNodeWithText("Usar como monitor").assertIsDisplayed()
        screenshot("24-monitor-landscape")
        click("Usar como monitor")
        waitFor("Monitor TYU activo")
        screenshot("25-monitor-active-landscape")
        click("Detener monitor")
        back()
        waitFor("Dispositivo X")
        screenshot("26-connected-landscape")
        compose.onNodeWithText("Bypass").assertIsDisplayed()
        compose.activityRule.scenario.onActivity {
            it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
        }
    }
}
