package com.tyu.app.backend

import android.app.Application
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.provider.OpenableColumns
import java.io.File
import java.util.UUID
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import org.json.JSONObject

data class BypassService(val session: String, val stream: String? = null)
data class BypassFile(val id: String, val name: String, val bytes: Long = 0, val total: Long = 0,
    val status: String = "Preparando", val path: String? = null, val incoming: Boolean = false)
data class BypassState(val services: Map<String, BypassService> = emptyMap(),
    val storageEnabled: Boolean = false, val files: List<BypassFile> = emptyList(),
    val error: String? = null, val lens: String = "Principal", val noise: Boolean = true)

/** Independent service state. Display sessions are never changed by this controller. */
class BypassController(private val app: Application, private val scope: CoroutineScope,
    private val peer: () -> String?, private val send: (String, String) -> Unit) {
    private val mutable = MutableStateFlow(BypassState())
    val state = mutable.asStateFlow()
    private val ownedSessions = java.util.Collections.synchronizedSet(mutableSetOf<String>())
    fun clearError() { mutable.update { it.copy(error = null) } }
    fun error(message: String) { mutable.update { it.copy(error = message) } }
    fun configure(lens: String? = null, noise: Boolean? = null) {
        mutable.update { it.copy(lens = lens ?: it.lens, noise = noise ?: it.noise) }
    }
    private fun command(kind: String, data: JSONObject = JSONObject()) {
        val id = peer() ?: return
        send(kind, data.put("peer", id).toString())
    }
    fun start(service: String) {
        if (state.value.services.containsKey(service)) return
        command("bypass-start", JSONObject().put("service", service))
    }
    fun stop(service: String) {
        val session = state.value.services[service]?.session ?: return
        command("bypass-stop", JSONObject().put("session", session))
        stopCapture(service)
        if (service == "Storage") setStorage(false)
    }
    fun media(service: String) {
        val session = state.value.services[service]?.session ?: return
        command("bypass-media", JSONObject().put("session", session).put("service", service))
    }
    fun setStorage(enabled: Boolean) {
        @Suppress("DEPRECATION")
        val root = if (enabled) Environment.getExternalStorageDirectory().absolutePath else ""
        send("storage-root", root)
        mutable.update { it.copy(storageEnabled = enabled) }
    }
    fun sendFiles(uris: List<Uri>) {
        val destination = peer() ?: return
        scope.launch(Dispatchers.IO) {
            for (uri in uris.take(32)) {
                val id = UUID.randomUUID().toString()
                var staging: File? = null
                try {
                    val name = app.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
                        if (it.moveToFirst()) it.getString(0) else null
                    } ?: "archivo"
                    val safe = name.replace(Regex("[\\\\/:*?\"<>|\\p{Cntrl}]"), "_").take(180).trim('.').ifEmpty { "archivo" }
                    val dir = File(app.cacheDir, "tyu-outgoing/$id").apply { check(mkdirs()) }
                    staging = File(dir, safe)
                    mutable.update { it.copy(files = it.files + BypassFile(id, safe, path = staging.absolutePath)) }
                    app.contentResolver.openInputStream(uri).use { input ->
                        checkNotNull(input) { "No se pudo leer $safe" }
                        staging.outputStream().use { output ->
                            val buffer = ByteArray(65536)
                            var size = 0L
                            while (true) {
                                ensureActive()
                                val n = input.read(buffer)
                                if (n < 0) break
                                size += n
                                check(size <= 8L * 1024 * 1024 * 1024) { "El archivo supera 8 GB" }
                                output.write(buffer, 0, n)
                            }
                        }
                    }
                    val file = staging
                    mutable.update { it.copy(files = it.files.map { f -> if (f.id == id) f.copy(total = file.length(), status = "Enviando") else f }) }
                    send("send-file", JSONObject().put("peer", destination).put("id", id).put("path", file.absolutePath).toString())
                } catch (e: Exception) {
                    staging?.delete(); staging?.parentFile?.delete()
                    if (e is CancellationException) throw e
                    mutable.update { it.copy(files = it.files.map { f -> if (f.id == id) f.copy(status = e.message ?: "Error") else f }, error = e.message) }
                }
            }
        }
    }
    fun cancelFile(id: String) = command("cancel-file", JSONObject().put("id", id))
    private fun publishReceived(file: BypassFile) = scope.launch(Dispatchers.IO) {
        try {
            val source = File(app.filesDir,"tyu-core/received/${file.name}")
            if (android.os.Build.VERSION.SDK_INT >= 29) {
                val values = android.content.ContentValues().apply {
                    put(android.provider.MediaStore.Downloads.DISPLAY_NAME,file.name)
                    put(android.provider.MediaStore.Downloads.RELATIVE_PATH,"Download/TYU")
                    put(android.provider.MediaStore.Downloads.IS_PENDING,1)
                }
                val uri = checkNotNull(app.contentResolver.insert(android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI,values))
                try {
                    app.contentResolver.openOutputStream(uri)!!.use { out -> source.inputStream().use { it.copyTo(out,65536) } }
                    values.clear(); values.put(android.provider.MediaStore.Downloads.IS_PENDING,0)
                    app.contentResolver.update(uri,values,null,null)
                } catch (e: Exception) { app.contentResolver.delete(uri,null,null); throw e }
            } else {
                @Suppress("DEPRECATION")
                val dir = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS),"TYU").apply {mkdirs()}
                source.copyTo(File(dir,file.name),overwrite=false)
            }
            source.delete()
            mutable.update { it.copy(files=it.files.map { f -> if(f.id==file.id) f.copy(status="Guardado en Descargas/TYU") else f }) }
        } catch (e: Exception) {
            if(e is CancellationException) throw e
            error("El archivo se recibió, pero no pudo guardarse en Descargas/TYU: ${e.message}")
        }
    }
    private fun stopCapture(service: String) {
        BypassCaptureService.stop(app, service)
    }
    fun close() {
        app.stopService(Intent(app, BypassCaptureService::class.java))
        setStorage(false)
        mutable.update { it.copy(services = emptyMap(), storageEnabled = false) }
    }
    /** Returns true only for events owned by Bypass, leaving Mirror/Monitor's handlers intact. */
    fun event(e: JSONObject): Boolean {
        when (e.getString("type")) {
            "service-started" -> {
                val kind = e.getString("service")
                ownedSessions.add(e.getString("session"))
                if (state.value.services[kind]?.session?.let { it != e.getString("session") } == true) {
                    command("bypass-stop", JSONObject().put("session", e.getString("session")))
                    return true
                }
                if (ownedSessions.size > 256) { ownedSessions.retainAll(state.value.services.values.map { it.session }.toSet() + e.getString("session")) }
                mutable.update { it.copy(services = it.services + (kind to BypassService(e.getString("session")))) }
                return true
            }
            "service-stopped" -> {
                val kind = state.value.services.entries.find { it.value.session == e.getString("session") }?.key ?: return true
                stopCapture(kind)
                if (kind == "Storage") setStorage(false)
                mutable.update { it.copy(services = it.services - kind) }
                return true
            }
            "media-started" -> {
                val kind = state.value.services.entries.find { it.value.session == e.getString("session") }?.key ?: return ownedSessions.contains(e.getString("session"))
                mutable.update { old -> old.copy(services = old.services + (kind to old.services.getValue(kind).copy(stream = e.getString("stream")))) }
                return true
            }
            "keyframe" -> {
                if (e.optString("session") != state.value.services["Camera"]?.session) return false
                BypassCaptureService.keyframe(); return true
            }
            "disconnected" -> { close(); return false }
            "file-offered" -> {
                val id = e.getString("id")
                mutable.update { old -> if (old.files.any { it.id == id }) old else old.copy(files = old.files + BypassFile(id,e.getString("name"),total=e.getLong("total"),status="Recibiendo",incoming=true)) }
                return true
            }
            "file-progress", "file-complete", "file-failed" -> {
                val id = e.getString("id")
                val type = e.getString("type")
                val file = state.value.files.find { it.id == id }
                if (type == "file-complete" || type == "file-failed") {
                    state.value.files.find { it.id == id }?.path?.let { path -> File(path).let { it.delete(); it.parentFile?.delete() } }
                }
                mutable.update { old -> old.copy(files = old.files.map { f -> if (f.id != id) f else when(type) {
                    "file-complete" -> f.copy(bytes = f.total, status = "Completado", path = null)
                    "file-failed" -> f.copy(status = e.optString("error", "Interrumpido"), path = null)
                    else -> f.copy(bytes = e.getLong("bytes"), total = e.getLong("total"), status = "Transfiriendo")
                } }) }
                if (type == "file-complete" && file?.incoming == true) publishReceived(file)
                return true
            }
        }
        return false
    }
}
