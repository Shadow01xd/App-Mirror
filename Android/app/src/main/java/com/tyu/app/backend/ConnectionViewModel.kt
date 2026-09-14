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
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.json.JSONObject

data class Peer(val id: String, val name: String, val endpoint: String = "", val trusted: Boolean = false,
    /** "USB" when reached over the tethered cable network, else "Wi-Fi". */
    val link: String = "Wi-Fi") {
    fun ui(connected: Boolean = false) = UiDevice(id, name, name, ip = endpoint.substringBeforeLast(':', ""),
        status = if (connected) UiDeviceStatus.Connected else UiDeviceStatus.Available,
        connectionQuality = if (connected) "Conectado por $link" else "Disponible", nearby = true)
}

/** Whether [address] belongs to this phone's USB tethering network (rndis/usb/ncm interface). */
private fun isUsbLink(address: String): Boolean {
    val peer = address.split('.').takeIf { it.size == 4 } ?: return false
    return runCatching {
        java.net.NetworkInterface.getNetworkInterfaces().toList().any { iface ->
            iface.name.startsWith("rndis") || iface.name.startsWith("usb") || iface.name.startsWith("ncm")
        } && java.net.NetworkInterface.getNetworkInterfaces().toList()
            .filter { it.name.startsWith("rndis") || it.name.startsWith("usb") || it.name.startsWith("ncm") }
            .flatMap { it.inetAddresses.toList() }
            .any { it.hostAddress?.split('.')?.take(3) == peer.take(3) }
    }.getOrDefault(false)
}
data class ConnectionUiState(
    val ready: Boolean = false, val scanning: Boolean = false,
    val peers: List<Peer> = emptyList(), val connected: Peer? = null,
    val connectingId: String? = null, val error: String? = null,
    val rttMicros: Long = 0, val lastPong: Long? = null,
    val mirrorSession: String? = null, val mirrorStream: String? = null, val mirrorPending: Boolean = false,
    val monitorSession: String? = null, val monitorStream: String? = null, val monitorPending: Boolean = false,
    /** The PC sent a touch but the accessibility service is not enabled yet. */
    val accessibilityMissing: Boolean = false,
    /** "TYU control táctil" is enabled in Accessibility, so PC touches can be replayed. */
    val touchControl: Boolean = false,
    /** A known PC was found over the USB cable; the user is asked before connecting. */
    val usbOffer: Peer? = null,
)

class ConnectionViewModel(application: Application) : AndroidViewModel(application) {
    private val mutable = MutableStateFlow(ConnectionUiState())
    val state = mutable.asStateFlow()
    private val mutex = Mutex()
    private var handle = 0L
    private val usbOffered = HashSet<String>()
    private var lastDiscovery = 0L
    /** Already inside the native mutex (event loop): issue a command without re-locking. */
    private fun sendDirect(kind: String, value: String) {
        val result = JSONObject(NativeCore.command(handle, kind, value))
        if (!result.has("error") && kind == "connect") {
            mutable.update { it.copy(connectingId = result.getString("peer"), error = null) }
        }
    }
    /** Encoded H.264 access units from the PC while a Monitor session is active; newest wins. */
    val monitorFrames = Channel<ByteArray>(capacity = 128, onBufferOverflow = BufferOverflow.DROP_OLDEST)
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
                        if (state.value.monitorSession != null) {
                            while (true) {
                                val frame = NativeCore.acquireMediaFrame(handle) ?: break
                                monitorFrames.trySend(frame)
                            }
                        }
                    }
                    val control = TouchInjectionService.instance != null
                    if (control != state.value.touchControl) mutable.update { it.copy(touchControl = control) }
                    // The cable network appears later (USB tethering toggled on): a quiet rescan
                    // while unconnected only refreshes the list, it never connects by itself.
                    if (state.value.connected == null && state.value.connectingId == null &&
                        System.currentTimeMillis() - lastDiscovery > 8_000) {
                        lastDiscovery = System.currentTimeMillis()
                        mutex.withLock { runCatching { NativeCore.command(handle, "discover", "") } }
                    }
                    delay(if (state.value.monitorSession != null) 16 else 50)
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
    fun accessibilityPrompted() { mutable.update { it.copy(accessibilityMissing = false) } }
    fun answerUsb(accept: Boolean) {
        val peer = state.value.usbOffer ?: return
        mutable.update { it.copy(usbOffer = null) }
        if (accept) connect(peer) else usbOffered.add(peer.id)
    }
    fun discover() = send("discover", "")
    fun pair(uri: String) = send("pair", uri.trim())
    fun connect(peer: Peer) {
        send("connect", peer.id)
    }
    fun disconnect() { (state.value.connected?.id ?: state.value.connectingId)?.let { send("disconnect", it) } }
    fun forget() { state.value.connected?.id?.let { send("forget", it) } }
    fun ping() { state.value.connected?.id?.let { send("ping", it) } }
    /** The Rust Core handle backing this session — [MirrorCaptureService] pushes encoded frames
     * into the same handle from its own thread; every native call is serialized on the Rust side. */
    val nativeHandle: Long get() = handle
    fun startMirror() {
        val peer = state.value.connected?.id ?: return
        mutable.update { it.copy(mirrorPending = true) }
        send("mirror", peer)
    }
    fun startMirrorMedia(width: Int, height: Int, fps: Int, bitrate: Int) {
        val peer = state.value.connected?.id ?: return
        val session = state.value.mirrorSession ?: return
        send(
            "media-start",
            JSONObject().put("peer", peer).put("session", session)
                .put("width", width).put("height", height).put("fps", fps).put("bitrate", bitrate).toString(),
        )
    }
    fun stopMirror() {
        val peer = state.value.connected?.id ?: return
        val session = state.value.mirrorSession ?: return
        send("stop-mode", JSONObject().put("peer", peer).put("session", session).toString())
    }
    fun startMonitor() {
        val peer = state.value.connected?.id ?: return
        mutable.update { it.copy(monitorPending = true) }
        send("monitor", peer)
    }
    fun stopMonitor() {
        val peer = state.value.connected?.id ?: return
        val session = state.value.monitorSession ?: return
        send("stop-mode", JSONObject().put("peer", peer).put("session", session).toString())
    }
    /** The decoder skipped ahead to cut latency; ask the PC for a fresh keyframe right away. */
    fun requestMonitorKeyframe() {
        val peer = state.value.connected?.id ?: return
        val session = state.value.monitorSession ?: return
        val stream = state.value.monitorStream ?: return
        send("keyframe", JSONObject().put("peer", peer).put("session", session).put("stream", stream).toString())
    }
    private fun send(kind: String, value: String) {
        viewModelScope.launch(Dispatchers.IO) {
            mutex.withLock {
                if (handle == 0L || !state.value.ready) { mutable.update { it.copy(error = "Core se está iniciando") }; return@withLock }
                val result = JSONObject(NativeCore.command(handle, kind, value))
                if (result.has("error")) {
                    mutable.update {
                        it.copy(
                            error = connectionError(result.getString("error")),
                            mirrorPending = if (kind == "mirror") false else it.mirrorPending,
                            monitorPending = if (kind == "monitor") false else it.monitorPending,
                        )
                    }
                    return@withLock
                }
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
                // Wi-Fi stays manual. A known PC reached over the USB cable is offered once;
                // the user decides whether to connect.
                val current = state.value
                val peer = current.peers.find { it.id == id }
                if (peer?.trusted == true && current.connected == null && current.connectingId == null &&
                    current.usbOffer == null && id !in usbOffered && isUsbLink(endpoint.substringBeforeLast(':'))) {
                    mutable.update { it.copy(usbOffer = peer.copy(link = "USB")) }
                }
            }
            "lost" -> mutable.update { it.copy(peers = it.peers.filter { p -> p.id != event.getString("id") || p.trusted }) }
            "connecting" -> mutable.update { it.copy(connectingId = event.getString("peer"), error = null) }
            "connected" -> {
                restorePeers()
                val id = event.getString("id")
                val link = if (isUsbLink(event.optString("address"))) "USB" else "Wi-Fi"
                val peer = (state.value.peers.find { it.id == id } ?: Peer(id, event.getString("name"), trusted = true)).copy(link = link)
                mutable.update { it.copy(connected = peer, connectingId = null, error = null, scanning = false) }
            }
            "disconnected" -> mutable.update {
                it.copy(
                    connected = null, connectingId = null, rttMicros = 0,
                    mirrorSession = null, mirrorStream = null, mirrorPending = false,
                    monitorSession = null, monitorPending = false,
                )
            }
            "mode-started" -> when (event.getString("mode")) {
                "Mirror" -> mutable.update { it.copy(mirrorSession = event.getString("session"), mirrorPending = false) }
                "Monitor" -> mutable.update { it.copy(monitorSession = event.getString("session"), monitorPending = false) }
            }
            "mode-stopped" -> mutable.update {
                val session = event.getString("session")
                it.copy(
                    mirrorSession = it.mirrorSession.takeUnless { s -> s == session },
                    mirrorStream = if (it.mirrorSession == session) null else it.mirrorStream,
                    monitorSession = it.monitorSession.takeUnless { s -> s == session },
                    monitorStream = if (it.monitorSession == session) null else it.monitorStream,
                )
            }
            "media-started" -> mutable.update {
                val session = event.getString("session")
                val stream = event.getString("stream")
                when (session) {
                    it.monitorSession -> it.copy(monitorStream = stream)
                    else -> it.copy(mirrorStream = stream)
                }
            }
            "keyframe" -> MirrorCaptureService.requestKeyframe()
            "input" -> {
                val payload = event.optJSONObject("event") ?: return
                val variant = payload.keys().asSequence().firstOrNull() ?: return
                val service = TouchInjectionService.instance
                if (service == null) { mutable.update { it.copy(accessibilityMissing = true) }; return }
                when (variant) {
                    "TouchDown", "TouchMove", "TouchUp" -> {
                        val kind = when (variant) { "TouchDown" -> "down"; "TouchMove" -> "move"; else -> "up" }
                        val touch = payload.getJSONObject(variant)
                        service.inject(kind, touch.getDouble("x").toFloat(), touch.getDouble("y").toFloat())
                    }
                    "TextInput" -> service.type(payload.getString(variant))
                    "KeyDown" -> service.key(payload.getJSONObject(variant).getInt("code"))
                }
            }
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
