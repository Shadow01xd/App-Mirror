package com.tyu.app.backend

import android.Manifest
import android.app.*
import android.content.*
import android.content.pm.ServiceInfo
import android.hardware.camera2.*
import android.media.*
import android.media.audiofx.NoiseSuppressor
import android.os.*
import com.tyu.app.R
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.flow.MutableStateFlow
import org.json.JSONObject

/** Camera2/AudioRecord capture for Bypass only. Never owns a MediaProjection. */
class BypassCaptureService : Service() {
    private data class Job(val running: AtomicBoolean, val thread: Thread)
    private val jobs = mutableMapOf<String, Job>()
    override fun onBind(intent: Intent?) = null
    override fun onCreate() { super.onCreate(); instance = this }
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent == null) { stopSelf(); return START_NOT_STICKY }
        val kind = intent.getStringExtra("kind") ?: return START_NOT_STICKY
        if (jobs.containsKey(kind)) return START_NOT_STICKY
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel("tyu-bypass", "Periféricos TYU", NotificationManager.IMPORTANCE_LOW))
        val notification = Notification.Builder(this, "tyu-bypass").setSmallIcon(R.drawable.ic_tyu)
            .setContentTitle("TYU · Bypass").setContentText("Compartiendo cámara o micrófono con tu PC por la red local")
            .setOngoing(true).build()
        if (Build.VERSION.SDK_INT >= 30) {
            val kinds = jobs.keys + kind
            val types = (if ("Camera" in kinds) ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA else 0) or
                (if ("Microphone" in kinds) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0)
            startForeground(52, notification, types)
        } else startForeground(52, notification)
        val running = AtomicBoolean(true)
        val thread = Thread({
            try {
                if (kind == "Camera") camera(intent, running) else microphone(intent, running)
            } catch (e: Exception) {
                if (running.get()) {
                    errors.value = "${if (kind == "Camera") "Cámara" else "Micrófono"}: ${e.message}"
                    val data = JSONObject().put("peer", intent.getStringExtra("peer")).put("session", intent.getStringExtra("session"))
                    NativeCore.command(intent.getLongExtra("handle",0), "bypass-stop", data.toString())
                }
            } finally { Handler(mainLooper).post { if (jobs[kind]?.running === running) stopJob(kind) } }
        }, "tyu-bypass-$kind")
        jobs[kind] = Job(running, thread)
        thread.start()
        return START_NOT_STICKY
    }
    private fun stopJob(kind: String) {
        jobs.remove(kind)?.running?.set(false)
        if (jobs.isEmpty()) { stopForeground(STOP_FOREGROUND_REMOVE); stopSelf() }
    }
    override fun onDestroy() {
        jobs.values.forEach { it.running.set(false) }
        jobs.clear(); instance = null
        super.onDestroy()
    }
    private fun send(i: Intent, id: Long, timestamp: Long, key: Boolean, data: ByteArray) = NativeCore.sendMediaFrame(
        i.getLongExtra("handle",0), i.getStringExtra("peer")!!, i.getStringExtra("session")!!,
        i.getStringExtra("stream")!!, id, timestamp, key, data)

    @android.annotation.SuppressLint("MissingPermission")
    private fun microphone(i: Intent, running: AtomicBoolean) {
        val size = maxOf(1920 * 4, AudioRecord.getMinBufferSize(48000, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT))
        val record = AudioRecord(MediaRecorder.AudioSource.VOICE_RECOGNITION,48000,AudioFormat.CHANNEL_IN_MONO,AudioFormat.ENCODING_PCM_16BIT,size)
        var suppression: NoiseSuppressor? = null
        try {
            check(record.state == AudioRecord.STATE_INITIALIZED) { "No se pudo abrir el micrófono" }
            if (i.getBooleanExtra("noise",true) && NoiseSuppressor.isAvailable()) {
                suppression = NoiseSuppressor.create(record.audioSessionId)?.apply { enabled = true }
            }
            record.startRecording()
            val buffer = ByteArray(1920) // 20 ms, mono S16LE at 48 kHz
            var frame = 0L
            while (running.get()) {
                val n = record.read(buffer,0,buffer.size,AudioRecord.READ_BLOCKING)
                check(n > 0) { "AudioRecord devolvió $n" }
                send(i,++frame,System.nanoTime()/1000,true,buffer.copyOf(n))
            }
        } finally { runCatching { record.stop() }; suppression?.release(); record.release() }
    }
    @android.annotation.SuppressLint("MissingPermission")
    private fun camera(i: Intent, running: AtomicBoolean) {
        val manager = getSystemService(CameraManager::class.java)
        val id = i.getStringExtra("cameraId") ?: error("Falta el perfil de cámara negociado")
        val width = i.getIntExtra("width",0)
        val height = i.getIntExtra("height",0)
        val fps = i.getIntExtra("fps",0)
        check((width == 1920 && height == 1080 || width == 1280 && height == 720) && fps in listOf(30,60)) { "Formato de cámara no válido" }
        val fpsRange = android.util.Range(i.getIntExtra("fpsLow",fps),i.getIntExtra("fpsHigh",fps))
        val codec = MediaCodec.createByCodecName(i.getStringExtra("encoder") ?: error("Falta el codificador de cámara"))
        val handlerThread = HandlerThread("tyu-bypass-camera2").apply { start() }
        val handler = Handler(handlerThread.looper)
        var surface: android.view.Surface? = null
        var device: CameraDevice? = null
        var session: CameraCaptureSession? = null
        try {
            val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC,width,height).apply {
                setInteger(MediaFormat.KEY_COLOR_FORMAT,MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
                setInteger(MediaFormat.KEY_BIT_RATE,i.getIntExtra("bitrate",8_000_000)); setInteger(MediaFormat.KEY_FRAME_RATE,fps)
                setInteger(MediaFormat.KEY_I_FRAME_INTERVAL,1)
            }
            codec.configure(format,null,null,MediaCodec.CONFIGURE_FLAG_ENCODE)
            val target = codec.createInputSurface(); surface = target; codec.start()
            val opened = CountDownLatch(1)
            var cameraError: String? = null
            manager.openCamera(id,object : CameraDevice.StateCallback() {
                override fun onOpened(camera: CameraDevice) { if (!running.get()) camera.close() else device = camera; opened.countDown() }
                override fun onDisconnected(camera: CameraDevice) { cameraError="Cámara desconectada"; camera.close(); opened.countDown() }
                override fun onError(camera: CameraDevice,error: Int) { cameraError="No se pudo abrir la cámara ($error)"; camera.close(); opened.countDown() }
            },handler)
            check(opened.await(5,TimeUnit.SECONDS) && device != null) { cameraError ?: "La cámara no responde" }
            val configured = CountDownLatch(1)
            @Suppress("DEPRECATION")
            device!!.createCaptureSession(listOf(target),object : CameraCaptureSession.StateCallback() {
                override fun onConfigured(s: CameraCaptureSession) {
                    if (!running.get()) { s.close(); configured.countDown(); return }
                    session=s
                    try {
                        val request = device!!.createCaptureRequest(CameraDevice.TEMPLATE_RECORD).apply {
                            addTarget(target); set(CaptureRequest.CONTROL_AF_MODE,CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_VIDEO)
                            set(CaptureRequest.CONTROL_AE_TARGET_FPS_RANGE,fpsRange)
                        }
                        s.setRepeatingRequest(request.build(),null,handler)
                    } catch (e: Exception) { cameraError=e.message }
                    configured.countDown()
                }
                override fun onConfigureFailed(s: CameraCaptureSession) { cameraError="No se pudo iniciar la cámara"; configured.countDown() }
            },handler)
            check(configured.await(5,TimeUnit.SECONDS) && session != null && cameraError == null) { cameraError ?: "No se pudo preparar la captura" }
            val info = MediaCodec.BufferInfo()
            var csd = byteArrayOf()
            var frame = 0L
            while (running.get()) {
                check(cameraError == null) { cameraError!! }
                if (needsKeyframe.getAndSet(false)) codec.setParameters(Bundle().apply { putInt(MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME,0) })
                val index = codec.dequeueOutputBuffer(info,10000)
                if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                    csd = (0..1).flatMap { n -> codec.outputFormat.getByteBuffer("csd-$n")?.let { b -> ByteArray(b.remaining()).also { b.get(it) }.toList() } ?: emptyList() }.toByteArray()
                } else if (index >= 0) {
                    try {
                        val buffer = codec.getOutputBuffer(index)!!
                        buffer.position(info.offset); buffer.limit(info.offset+info.size)
                        val bytes = ByteArray(info.size).also { buffer.get(it) }
                        if (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0) csd=bytes
                        else if (bytes.isNotEmpty()) {
                            val key = info.flags and MediaCodec.BUFFER_FLAG_KEY_FRAME != 0
                            if (!send(i,++frame,info.presentationTimeUs,key,if(key) csd+bytes else bytes)) needsKeyframe.set(true)
                        }
                    } finally { codec.releaseOutputBuffer(index,false) }
                }
            }
        } finally {
            running.set(false); session?.close(); device?.close()
            runCatching { codec.stop() }; codec.release(); surface?.release()
            handlerThread.quitSafely(); handlerThread.join(1000)
        }
    }
    companion object {
        @Volatile private var instance: BypassCaptureService? = null
        val errors = MutableStateFlow<String?>(null)
        private val needsKeyframe = AtomicBoolean(false)
        fun keyframe() { needsKeyframe.set(true) }
        fun stop(context: Context, kind: String) { instance?.let { service -> Handler(service.mainLooper).post { service.stopJob(kind) } } }
    }
}
