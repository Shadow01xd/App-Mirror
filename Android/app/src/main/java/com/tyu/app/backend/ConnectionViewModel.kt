package com.tyu.app.backend

import android.app.Application
import android.content.Context
import android.net.wifi.WifiManager
import android.os.Build
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.tyu.app.model.UiDevice
import com.tyu.app.model.UiDeviceStatus
import java.io.File
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.json.JSONObject

data class Peer(val id: String, val name: String, val endpoint: String = "", val trusted: Boolean = false) {
    fun ui(connected: Boolean = false) = UiDevice(id, name, name, ip = endpoint.substringBeforeLast(':', ""),
        status = if (connected) UiDeviceStatus.Connected else UiDeviceStatus.Available,
        connectionQuality = if (connected) "TyuLink autenticado" else "Disponible", nearby = true)
}
data class ConnectionUiState(
    val ready: Boolean = false, val scanning: Boolean = false,
    val peers: List<Peer> = emptyList(), val connected: Peer? = null,
    val connectingId: String? = null, val error: String? = null,
    val rttMicros: Long = 0, val lastPong: Long? = null,
)

class ConnectionViewModel(application: Application) : AndroidViewModel(application) {
    private val mutable = MutableStateFlow(ConnectionUiState())
    val state = mutable.asStateFlow()
    private val mutex = Mutex()
    private var handle = 0L
    private var lock: WifiManager.MulticastLock? = null

    init {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val directory = File(application.filesDir, "tyu-core").apply { mkdirs() }
                mutex.withLock {
                    handle = NativeCore.initialize(directory.absolutePath, Build.MODEL.take(100))
                    check(handle != 0L) { "No se pudo abrir la identidad local" }
                    check(NativeCore.start(handle) == 0) { "No se pudo iniciar TYU Core" }
                    restorePeers()
                }
                val wifi = application.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
                lock = wifi?.createMulticastLock("tyu-discovery")?.apply { setReferenceCounted(false); acquire() }
                mutable.update { it.copy(ready = true) }
                discover()
                while (isActive) {
                    mutex.withLock {
                        repeat(32) {
                            val text = NativeCore.poll(handle)
                            if (text != "null") applyEvent(JSONObject(text))
                        }
                    }
                    delay(50)
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                mutable.update { it.copy(error = error.message ?: "Backend nativo no disponible", ready = false) }
            } finally {
                withContext(NonCancellable + Dispatchers.IO) {
                    mutex.withLock {
                        if (handle != 0L) { NativeCore.shutdown(handle); handle = 0L }
                        if (lock?.isHeld == true) lock?.release()
                    }
                }
            }
        }
    }

    private fun restorePeers() {
        val json = JSONObject(NativeCore.snapshot(handle))
        val peers = json.optJSONArray("peers") ?: return
        val restored = (0 until peers.length()).map { i -> peers.getJSONObject(i).let {
            Peer(it.getString("id"), it.getString("name"), it.optString("endpoint"), true)
        } }
        mutable.update { it.copy(peers = restored) }
    }
    fun clearError() { mutable.update { it.copy(error = null) } }
    fun discover() = send("discover", "")
    fun pair(uri: String) = send("pair", uri.trim())
    fun connect(peer: Peer) {
        send("connect", peer.id)
    }
    fun disconnect() { (state.value.connected?.id ?: state.value.connectingId)?.let { send("disconnect", it) } }
    fun forget() { state.value.connected?.id?.let { send("forget", it) } }
    fun ping() { state.value.connected?.id?.let { send("ping", it) } }
    private fun send(kind: String, value: String) {
        viewModelScope.launch(Dispatchers.IO) {
            mutex.withLock {
                if (handle == 0L || !state.value.ready) { mutable.update { it.copy(error = "Core se está iniciando") }; return@withLock }
                val result = JSONObject(NativeCore.command(handle, kind, value))
                if (result.has("error")) { mutable.update { it.copy(error = connectionError(result.getString("error"))) }; return@withLock }
                when (kind) {
                    "pair", "connect" -> mutable.update { it.copy(connectingId = result.getString("peer"), error = null) }
                    "discover" -> mutable.update { it.copy(scanning = true, error = null) }
                    "forget" -> mutable.update { it.copy(peers = it.peers.filterNot { p -> p.id == value }) }
                }
            }
        }
    }
    private fun applyEvent(event: JSONObject) {
        when (event.getString("type")) {
            "found" -> {
                val id = event.getString("id")
                val endpoint = event.optJSONArray("endpoints")?.optString(0) ?: ""
                mutable.update { old ->
                    val existing = old.peers.find { it.id == id }
                    val peer = existing?.copy(endpoint = endpoint) ?: Peer(id, "TYU ${id.take(8)}", endpoint)
                    old.copy(peers = old.peers.filterNot { it.id == id } + peer, scanning = false)
                }
            }
            "lost" -> mutable.update { it.copy(peers = it.peers.filter { p -> p.id != event.getString("id") || p.trusted }) }
            "connecting" -> mutable.update { it.copy(connectingId = event.getString("peer"), error = null) }
            "connected" -> {
                restorePeers()
                val id = event.getString("id")
                val peer = state.value.peers.find { it.id == id } ?: Peer(id, event.getString("name"), trusted = true)
                mutable.update { it.copy(connected = peer, connectingId = null, error = null, scanning = false) }
            }
            "disconnected" -> mutable.update { it.copy(connected = null, connectingId = null, rttMicros = 0) }
            "failed" -> mutable.update { it.copy(connected = null, connectingId = null,
                error = event.getString("error").takeUnless { message -> message == "operation cancelled" }?.let(::connectionError)) }
            "error" -> mutable.update { it.copy(connectingId = null, error = connectionError(event.getString("error"))) }
            "metrics" -> mutable.update { it.copy(rttMicros = event.optLong("rttMicros")) }
            "pong" -> mutable.update { it.copy(lastPong = event.getLong("value")) }
        }
    }

    private fun connectionError(message: String): String = when (message) {
        "operation timed out" -> "No se pudo completar la conexión. Comprueba que TYU esté abierto en el PC, que ambos estén en la misma red y acepta la solicitud cuando aparezca."
        "request rejected" -> "El PC rechazó la solicitud. Puedes volver a intentarlo."
        "pairing ticket expired", "pairing ticket already used" -> "El QR caducó o ya se utilizó. Escanea el código actual del PC o conecta desde la búsqueda."
        "not available" -> "El equipo ya no está disponible. Busca de nuevo e inténtalo otra vez."
        "authentication failed" -> "No se pudo verificar la identidad del equipo."
        "peer revoked" -> "Se revocó la confianza en este equipo."
        else -> message
    }
}
