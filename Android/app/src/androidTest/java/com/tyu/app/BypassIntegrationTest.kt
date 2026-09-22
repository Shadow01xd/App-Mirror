package com.tyu.app

import android.content.ContentValues
import android.graphics.Bitmap
import android.os.Environment
import android.provider.MediaStore
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStoreOwner
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import com.tyu.app.backend.ConnectionViewModel
import org.junit.Rule
import org.junit.Test
import org.junit.Assert.*
import java.io.File

/** Run against the explicit bypass_probe example with camera/mic/storage grants on a test device. */
class BypassIntegrationTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    @Test fun real_storage_transfer_camera_and_microphone() {
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val context=instrumentation.targetContext
        val uri=InstrumentationRegistry.getArguments().getString("tyuPairingUri")
        org.junit.Assume.assumeTrue("Requires the explicit local bypass_probe host",uri != null && android.os.Build.VERSION.SDK_INT >= 29)
        lateinit var backend: ConnectionViewModel
        compose.activityRule.scenario.onActivity {
            backend=ViewModelProvider(
                it.application as ViewModelStoreOwner,
                ViewModelProvider.AndroidViewModelFactory.getInstance(it.application),
            )[ConnectionViewModel::class.java]
        }
        compose.waitUntil(15000) {backend.state.value.ready}
        backend.pair(requireNotNull(uri))
        compose.waitUntil(30000) {backend.state.value.connected != null}
        compose.waitUntil(10000) {compose.onAllNodesWithText("Bypass").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithText("Bypass").performClick()
        val fixture=File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOCUMENTS),"TYUBypassValidation").apply {mkdirs()}
        File(fixture,"phone.txt").writeText("real Android storage via TYU")
        val values=ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME,"tyu-bypass-sent-${System.currentTimeMillis()}.txt")
            put(MediaStore.Downloads.MIME_TYPE,"text/plain")
            put(MediaStore.Downloads.RELATIVE_PATH,"Download/TYUValidation")
        }
        val file=context.contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI,values)!!
        try {
            context.contentResolver.openOutputStream(file)!!.use {it.write("real Android transfer via TYU".toByteArray())}
            backend.bypass.start("Storage")
            compose.waitUntil(15000) {backend.bypass.state.value.storageEnabled}
            backend.bypass.sendFiles(listOf(file))
            compose.waitUntil(30000) {backend.bypass.state.value.files.any {it.status=="Completado"}}
            backend.bypass.start("Camera")
            compose.waitUntil(15000) {backend.bypass.state.value.services["Camera"]?.stream != null}
            backend.bypass.start("Microphone")
            compose.waitUntil(15000) {backend.bypass.state.value.services["Microphone"]?.stream != null}
            compose.waitForIdle()
            val screenshot=instrumentation.uiAutomation.takeScreenshot()
            File(context.getExternalFilesDir(null),"bypass-live.png").outputStream().use {screenshot.compress(Bitmap.CompressFormat.PNG,100,it)}
            screenshot.recycle()
            // Leave time for Windows Explorer/WebClient and a DirectShow consumer to exercise both endpoints.
            val end=System.currentTimeMillis()+60000
            while(System.currentTimeMillis()<end) {
                assertNull(backend.bypass.state.value.error)
                Thread.sleep(500)
            }
            assertEquals("written from Windows",File(fixture,"pc.txt").readText())
            assertNull(backend.state.value.mirrorSession)
            assertNull(backend.state.value.monitorSession)
        } finally {
            backend.bypass.stop("Camera");backend.bypass.stop("Microphone");backend.bypass.stop("Storage")
            backend.disconnect()
            context.contentResolver.delete(file,null,null)
            File(fixture,"phone.txt").delete();File(fixture,"pc.txt").delete();fixture.delete()
        }
    }
}
