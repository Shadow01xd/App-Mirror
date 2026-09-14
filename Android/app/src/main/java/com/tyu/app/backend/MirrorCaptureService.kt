package com.tyu.app.backend

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import androidx.core.app.NotificationCompat
import com.tyu.app.R
import kotlin.concurrent.thread

/**
 * Foreground service required by Android 14+ to hold a MediaProjection off the requesting
 * Activity. Captures this device's screen into an H.264 encoder Surface and pushes each
 * encoded access unit to the native Core via [NativeCore.sendMediaFrame] — the Core then
 * packetizes and sends it over the already-negotiated Mirror media stream.
 */
class MirrorCaptureService : Service() {
    private var projection: MediaProjection? = null
    private var virtualDisplay: VirtualDisplay? = null
    private var codec: MediaCodec? = null
    private var worker: Thread? = null
    @Volatile private var running = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(NOTIFICATION_ID, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION)
        } else {
            startForeground(NOTIFICATION_ID, notification())
        }
        val i = intent
        val data = i?.let {
            if (Build.VERSION.SDK_INT >= 33) it.getParcelableExtra(EXTRA_DATA, Intent::class.java)
            else @Suppress("DEPRECATION") it.getParcelableExtra(EXTRA_DATA)
        }
        val handle = i?.getLongExtra(EXTRA_HANDLE, 0L) ?: 0L
        val peer = i?.getStringExtra(EXTRA_PEER)
        val session = i?.getStringExtra(EXTRA_SESSION)
        val stream = i?.getStringExtra(EXTRA_STREAM)
        val resultCode = i?.getIntExtra(EXTRA_RESULT_CODE, 0) ?: 0
        if (data == null || handle == 0L || peer == null || session == null || stream == null) {
            stopSelf(startId)
            return START_NOT_STICKY
        }
        val width = i.getIntExtra(EXTRA_WIDTH, 720)
        val height = i.getIntExtra(EXTRA_HEIGHT, 1600)
        val fps = i.getIntExtra(EXTRA_FPS, 30)
        val bitrate = i.getIntExtra(EXTRA_BITRATE, 4_000_000)
        val density = i.getIntExtra(EXTRA_DENSITY, 320)

        val manager = getSystemService(MEDIA_PROJECTION_SERVICE) as MediaProjectionManager
        val proj = manager.getMediaProjection(resultCode, data)
        if (proj == null) {
            stopSelf(startId)
            return START_NOT_STICKY
        }
        projection = proj
        // Required since Android 14: createVirtualDisplay throws without a registered callback.
        proj.registerCallback(object : MediaProjection.Callback() {
            override fun onStop() {
                running = false
            }
        }, Handler(Looper.getMainLooper()))

        val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
            setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
            setInteger(MediaFormat.KEY_BIT_RATE, bitrate)
            setInteger(MediaFormat.KEY_FRAME_RATE, fps)
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
            if (Build.VERSION.SDK_INT >= 29) setInteger(MediaFormat.KEY_PREPEND_HEADER_TO_SYNC_FRAMES, 1)
            if (Build.VERSION.SDK_INT >= 30) setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
        }
        val enc = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
        enc.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
        val surface = enc.createInputSurface()
        enc.start()
        codec = enc
        activeCodec = enc

        virtualDisplay = proj.createVirtualDisplay(
            "TyuMirror", width, height, density,
            DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, surface, null, null,
        )

        running = true
        worker = thread(name = "tyu-mirror-encode") {
            val info = MediaCodec.BufferInfo()
            var frameId = 0L
            // SPS/PPS arrive once as a CODEC_CONFIG buffer; the PC decoder needs them in front of
            // every keyframe, so cache and prepend rather than drop them.
            var codecConfig: ByteArray? = null
            while (running) {
                val index = try { enc.dequeueOutputBuffer(info, 100_000) } catch (e: IllegalStateException) { break }
                if (index >= 0) {
                    if (info.size > 0) {
                        enc.getOutputBuffer(index)?.let { buffer ->
                            buffer.position(info.offset)
                            buffer.limit(info.offset + info.size)
                            val bytes = ByteArray(info.size)
                            buffer.get(bytes)
                            if ((info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG) != 0) {
                                codecConfig = bytes
                            } else {
                                val keyframe = (info.flags and MediaCodec.BUFFER_FLAG_KEY_FRAME) != 0
                                val config = codecConfig
                                val payload = if (keyframe && config != null && !bytes.startsWithSps()) config + bytes else bytes
                                NativeCore.sendMediaFrame(handle, peer, session, stream, frameId++, info.presentationTimeUs, keyframe, payload)
                            }
                        }
                    }
                    enc.releaseOutputBuffer(index, false)
                }
            }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        running = false
        activeCodec = null
        worker?.join(500)
        worker = null
        runCatching { codec?.stop() }
        runCatching { codec?.release() }
        codec = null
        virtualDisplay?.release()
        virtualDisplay = null
        projection?.stop()
        projection = null
        super.onDestroy()
    }

    private fun notification(): Notification {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val manager = getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Espejo TYU", NotificationManager.IMPORTANCE_LOW),
            )
        }
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("TYU")
            .setContentText("Transmitiendo pantalla a la PC")
            .setSmallIcon(R.drawable.ic_tyu)
            .setOngoing(true)
            .build()
    }

    private fun ByteArray.startsWithSps(): Boolean {
        val start = indexOfStartCode(this) ?: return false
        return start + 3 < size && (this[start + 3].toInt() and 0x1F) == 7
    }
    private fun indexOfStartCode(bytes: ByteArray): Int? {
        for (i in 0 until bytes.size - 2) {
            if (bytes[i].toInt() == 0 && bytes[i + 1].toInt() == 0 && bytes[i + 2].toInt() == 1) return i
        }
        return null
    }

    companion object {
        @Volatile private var activeCodec: MediaCodec? = null
        /** The PC lost a frame: emit a sync frame now instead of waiting for the next GOP. */
        fun requestKeyframe() {
            activeCodec?.let { codec ->
                runCatching { codec.setParameters(Bundle().apply { putInt(MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME, 0) }) }
            }
        }
        private const val NOTIFICATION_ID = 4177
        private const val CHANNEL_ID = "tyu-mirror"
        const val EXTRA_RESULT_CODE = "resultCode"
        const val EXTRA_DATA = "data"
        const val EXTRA_HANDLE = "handle"
        const val EXTRA_PEER = "peer"
        const val EXTRA_SESSION = "session"
        const val EXTRA_STREAM = "stream"
        const val EXTRA_WIDTH = "width"
        const val EXTRA_HEIGHT = "height"
        const val EXTRA_FPS = "fps"
        const val EXTRA_BITRATE = "bitrate"
        const val EXTRA_DENSITY = "density"
    }
}
