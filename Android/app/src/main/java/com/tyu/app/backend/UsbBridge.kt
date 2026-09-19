package com.tyu.app.backend

import java.io.DataInputStream
import java.io.DataOutputStream
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * Lets a PC reach this phone's TYU node straight through the USB cable, the way Android Studio
 * does: the PC runs `adb forward tcp:<local> tcp:47810`, which lands on this server.
 *
 * TYU speaks QUIC, which is UDP, and adb only tunnels TCP. So each QUIC packet travels the cable
 * with a 2-byte length in front and is handed back to UDP on the other side. Over a cable there
 * is effectively no loss, so wrapping QUIC in a reliable stream costs nothing in practice.
 *
 * The socket listens on loopback only: nothing on the network can reach it, and a caller still
 * has to pass TYU's own pairing and TLS checks to do anything.
 */
class UsbBridge(private val scope: CoroutineScope, private val local: () -> Pair<String, Int>?) {
    private var server: ServerSocket? = null

    fun start() {
        if (server != null) return
        scope.launch(Dispatchers.IO) {
            val socket = runCatching { ServerSocket(PORT, 4, InetAddress.getByName("127.0.0.1")) }
                .getOrElse { return@launch }
            server = socket
            try {
                while (isActive && !socket.isClosed) {
                    val client = runCatching { socket.accept() }.getOrNull() ?: break
                    scope.launch(Dispatchers.IO) { serve(client) }
                }
            } finally { runCatching { socket.close() }; server = null }
        }
    }

    fun stop() {
        runCatching { server?.close() }
        server = null
    }

    private fun serve(client: Socket) {
        // No identity yet (core still starting): hang up rather than tunnel into nothing.
        val (id, port) = local() ?: run { runCatching { client.close() }; return }
        val node = DatagramSocket()
        try {
            client.tcpNoDelay = true
            val input = DataInputStream(client.getInputStream().buffered())
            val output = DataOutputStream(client.getOutputStream().buffered())
            // Greeting: who this phone is, so the PC knows which paired device it just reached.
            val greeting = id.toByteArray()
            synchronized(output) {
                output.writeShort(greeting.size)
                output.write(greeting)
                output.flush()
            }
            val target = InetAddress.getByName("127.0.0.1")
            // Node -> PC: whatever the local TYU node answers goes back up the cable.
            val upstream = Thread {
                val buffer = ByteArray(MAX_PACKET)
                try {
                    while (true) {
                        val packet = DatagramPacket(buffer, buffer.size)
                        node.receive(packet)
                        synchronized(output) {
                            output.writeShort(packet.length)
                            output.write(packet.data, packet.offset, packet.length)
                            output.flush()
                        }
                    }
                } catch (_: Exception) { runCatching { client.close() } }
            }
            upstream.isDaemon = true
            upstream.start()
            // PC -> node.
            val buffer = ByteArray(MAX_PACKET)
            while (true) {
                val size = input.readUnsignedShort()
                if (size > MAX_PACKET) break
                input.readFully(buffer, 0, size)
                node.send(DatagramPacket(buffer, size, target, port))
            }
        } catch (_: Exception) {
        } finally {
            runCatching { node.close() }
            runCatching { client.close() }
        }
    }

    companion object {
        /** Port the PC forwards onto with `adb forward`. */
        const val PORT = 47810
        /** A QUIC packet never exceeds this; anything bigger is a desynchronised stream. */
        private const val MAX_PACKET = 2048
    }
}
