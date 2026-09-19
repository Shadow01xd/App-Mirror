package com.tyu.app.backend

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.*
import androidx.compose.ui.platform.LocalContext
import com.tyu.app.ui.components.TyuDialog

@Composable
fun BypassEffects(backend: ConnectionViewModel) {
    val context = LocalContext.current
    val state by backend.bypass.state.collectAsState()
    val captureError by BypassCaptureService.errors.collectAsState()
    var pending by remember { mutableStateOf<String?>(null) }
    val requested = remember { mutableStateMapOf<String, Boolean>() }
    fun grant(kind: String, allowed: Boolean) {
        if (kind !in backend.bypass.state.value.services) { pending = null; return }
        if (allowed) {
            if (kind == "Storage") backend.bypass.setStorage(true) else backend.bypass.media(kind)
        } else {
            backend.bypass.error("Permiso denegado. Vuelve a activar el servicio para concederlo.")
            backend.bypass.stop(kind)
        }
        pending = null
    }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { result ->
        pending?.let { grant(it, result.values.all { granted -> granted }) }
    }
    val storage = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        grant("Storage", Build.VERSION.SDK_INT >= 30 && Environment.isExternalStorageManager())
    }
    LaunchedEffect(captureError) {
        captureError?.let { backend.bypass.error(it); BypassCaptureService.errors.value = null }
    }
    LaunchedEffect(state.services, pending) {
        requested.keys.toList().filter { key -> state.services.values.none { it.session == key } }.forEach { requested.remove(it) }
        if (pending != null) return@LaunchedEffect
        val (kind, service) = state.services.entries.firstOrNull { it.value.session !in requested } ?: return@LaunchedEffect
        requested[service.session] = true
        pending = kind
        if (kind == "Storage" && Build.VERSION.SDK_INT >= 30) {
            if (Environment.isExternalStorageManager()) grant(kind,true)
            else runCatching { storage.launch(Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,Uri.parse("package:${context.packageName}"))) }
                .onFailure { grant(kind,false) }
        } else {
            val permissions = when(kind) {
                "Camera" -> arrayOf(Manifest.permission.CAMERA)
                "Microphone" -> arrayOf(Manifest.permission.RECORD_AUDIO)
                "Storage" -> arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE,Manifest.permission.WRITE_EXTERNAL_STORAGE)
                else -> emptyArray()
            }
            if (permissions.all { context.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }) grant(kind,true)
            else permission.launch(permissions)
        }
    }
    state.services.filterKeys { it == "Camera" || it == "Microphone" }.forEach { (kind, service) ->
        key(kind) { LaunchedEffect(service.stream) {
            val stream = service.stream ?: return@LaunchedEffect
            val peer = backend.state.value.connected?.id ?: return@LaunchedEffect
            try {
                context.startForegroundService(Intent(context, BypassCaptureService::class.java).apply {
                    putExtra("kind",kind); putExtra("handle",backend.nativeHandle); putExtra("peer",peer)
                    putExtra("session",service.session); putExtra("stream",stream)
                    putExtra("lens",state.lens); putExtra("noise",state.noise)
                })
            } catch (e: Exception) { backend.bypass.error(e.message ?: "No se pudo activar el servicio"); backend.bypass.stop(kind) }
        } }
    }
    state.error?.let { TyuDialog("Bypass",it,"Aceptar",backend.bypass::clearError,backend.bypass::clearError) }
}
