package com.tyu.app.feature.monitor

import android.media.MediaCodec
import android.media.MediaFormat
import android.view.Surface
import android.view.SurfaceHolder
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.launch

/**
 * Renders the PC screen for Monitor mode: H.264 access units from [frames] go straight into a
 * hardware decoder whose output surface is this view. Decoding waits for the first keyframe that
 * carries SPS/PPS, which the PC guarantees on every keyframe.
 */
@Composable
fun MonitorVideo(frames: ReceiveChannel<ByteArray>, modifier: Modifier = Modifier, onNeedKeyframe: () -> Unit = {}) {
    val player = remember { MonitorPlayer(frames, onNeedKeyframe) }
    DisposableEffect(player) { onDispose { player.release() } }
    AndroidView(modifier = modifier, factory = { context ->
        AspectSurfaceView(context).apply {
            player.onVideoSize = ::setVideoSize
            holder.addCallback(object : SurfaceHolder.Callback {
                override fun surfaceCreated(holder: SurfaceHolder) { player.start(holder.surface) }
                override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {}
                override fun surfaceDestroyed(holder: SurfaceHolder) { player.stop() }
            })
        }
    })
}

private class MonitorPlayer(private val frames: ReceiveChannel<ByteArray>, private val onNeedKeyframe: () -> Unit) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var job: Job? = null
    var onVideoSize: (Int, Int) -> Unit = { _, _ -> }

    fun start(surface: Surface) {
        stop()
        job = scope.launch {
            var codec: MediaCodec? = null
            val info = MediaCodec.BufferInfo()
            var presentation = 0L
            // After skipping ahead, P-frames whose references were dropped paint garbage; hold
            // everything until the requested keyframe arrives.
            var waitingForKeyframe = false
            try {
                for (first in frames) {
                    val batch = catchUp(first)
                    if (batch.isEmpty()) waitingForKeyframe = true
                    for (frame in batch) {
                        if (waitingForKeyframe) {
                            if (!frame.isKeyframe()) continue
                            waitingForKeyframe = false
                        }
                        if (codec == null) {
                            if (!frame.isKeyframe()) continue // need SPS/PPS before the decoder can configure
                            codec = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC).apply {
                                // Dimensions are a hint only; the decoder reads the real ones from SPS.
                                val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, 1280, 720)
                                if (android.os.Build.VERSION.SDK_INT >= 30) format.setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                                configure(format, surface, null, 0)
                                start()
                            }
                        }
                        val dec = codec
                        val input = dec.dequeueInputBuffer(20_000)
                        if (input >= 0) {
                            dec.getInputBuffer(input)?.apply { clear(); put(frame) }
                            dec.queueInputBuffer(input, 0, frame.size, presentation, 0)
                            presentation += 16_667
                        }
                        var output = dec.dequeueOutputBuffer(info, 0)
                        while (output >= 0 || output == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                            if (output == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                                val f = dec.outputFormat
                                onVideoSize(f.getInteger(MediaFormat.KEY_WIDTH), f.getInteger(MediaFormat.KEY_HEIGHT))
                            } else {
                                dec.releaseOutputBuffer(output, true)
                            }
                            output = dec.dequeueOutputBuffer(info, 0)
                        }
                    }
                }
            } catch (_: Exception) {
                // Surface gone or codec error: the session UI still shows "sin señal" and a
                // reconnect creates a fresh player.
            } finally {
                runCatching { codec?.stop() }
                runCatching { codec?.release() }
            }
        }
    }

    /**
     * Latency control: if frames piled up while decoding, jump to the newest keyframe among them
     * (or drop them all and ask the PC for one) instead of playing seconds behind.
     */
    private fun catchUp(first: ByteArray): List<ByteArray> {
        val pending = mutableListOf(first)
        while (true) pending.add(frames.tryReceive().getOrNull() ?: break)
        if (pending.size <= 6) return pending
        val key = pending.indexOfLast { it.isKeyframe() }
        if (key >= 0) return pending.subList(key, pending.size)
        onNeedKeyframe()
        return emptyList()
    }

    fun stop() { job?.cancel(); job = null }
    fun release() { stop(); scope.cancel() }

    /** IDR slice (5) or SPS (7) present: a point the decoder can (re)start from. */
    private fun ByteArray.isKeyframe(): Boolean {
        for (i in 0 until size - 3) {
            if (this[i].toInt() == 0 && this[i + 1].toInt() == 0 && this[i + 2].toInt() == 1) {
                val type = this[i + 3].toInt() and 0x1F
                if (type == 5 || type == 7) return true
            }
        }
        return false
    }
}
