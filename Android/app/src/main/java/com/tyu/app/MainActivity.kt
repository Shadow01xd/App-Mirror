package com.tyu.app

import android.os.Bundle
import android.content.Intent
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import kotlinx.coroutines.flow.MutableStateFlow

class MainActivity : ComponentActivity() {
    private val pairingUri = MutableStateFlow<String?>(null)
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        readPairingIntent(intent)
        setContent {
            val uri by pairingUri.collectAsState()
            TyuApp(fixture = BuildConfig.DEBUG && intent.getBooleanExtra("tyu.fixture", false), pairingUri = uri)
        }
    }
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        readPairingIntent(intent)
    }
    private fun readPairingIntent(intent: Intent) {
        val uri = intent.data
        if (uri?.scheme == "tyu" && uri.host == "pair") pairingUri.value = uri.toString()
    }
}
