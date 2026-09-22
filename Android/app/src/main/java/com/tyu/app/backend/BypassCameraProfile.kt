package com.tyu.app.backend

import android.content.Context
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraManager
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Build
import android.util.Range

/** A profile supported by both the selected lens and an AVC encoder, Bypass only. */
data class BypassCameraProfile(
    val cameraId: String, val width: Int, val height: Int, val fps: Int,
    val fpsRange: Range<Int>, val encoder: String, val bitrate: Int,
) {
    companion object {
        fun select(context: Context, lens: String): BypassCameraProfile {
            val manager = context.getSystemService(CameraManager::class.java)
            val facing = if (lens == "Frontal") CameraCharacteristics.LENS_FACING_FRONT else CameraCharacteristics.LENS_FACING_BACK
            val id = manager.cameraIdList.firstOrNull { manager.getCameraCharacteristics(it).get(CameraCharacteristics.LENS_FACING) == facing }
                ?: error("No se encontró esa cámara")
            val characteristics = manager.getCameraCharacteristics(id)
            val map = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP)
                ?: error("No se pudieron consultar los formatos de la cámara")
            val sizes = map.getOutputSizes(MediaCodec::class.java).orEmpty()
            val ranges = characteristics.get(CameraCharacteristics.CONTROL_AE_AVAILABLE_TARGET_FPS_RANGES).orEmpty()
            val encoders = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos
                .filter { it.isEncoder && it.supportedTypes.any { mime -> mime.equals(MediaFormat.MIMETYPE_VIDEO_AVC, ignoreCase = true) } }
                .sortedBy { if (Build.VERSION.SDK_INT >= 29) !it.isHardwareAccelerated else it.name.startsWith("OMX.google") || it.name.startsWith("c2.android") }
            for ((width, height, fps) in listOf(Triple(1920,1080,60), Triple(1920,1080,30), Triple(1280,720,60), Triple(1280,720,30))) {
                val size = sizes.firstOrNull { it.width == width && it.height == height } ?: continue
                val duration = map.getOutputMinFrameDuration(MediaCodec::class.java, size)
                if (duration > 0 && duration > 1_000_000_000L / fps + 1000L) continue
                val range = ranges.filter { it.upper == fps && it.lower <= fps }.maxByOrNull { it.lower } ?: continue
                for (encoder in encoders) {
                    val caps = runCatching { encoder.getCapabilitiesForType(MediaFormat.MIMETYPE_VIDEO_AVC) }.getOrNull() ?: continue
                    if (MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface !in caps.colorFormats) continue
                    val video = caps.videoCapabilities ?: continue
                    if (!video.areSizeAndRateSupported(width, height, fps.toDouble())) continue
                    val bitrate = (if (width == 1920) if (fps == 60) 12_000_000 else 8_000_000 else 4_000_000)
                        .coerceIn(video.bitrateRange.lower, video.bitrateRange.upper)
                    return BypassCameraProfile(id,width,height,fps,range,encoder.name,bitrate)
                }
            }
            error("La cámara y el codificador no admiten 720p o 1080p a 30/60 fps")
        }
    }
}
