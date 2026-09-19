package com.tyu.app

import android.app.Application
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner

/**
 * Owns the connection for as long as the app is alive.
 *
 * A ViewModel scoped to the Activity dies when that Activity does, which meant leaving the
 * screen tore the link down — closing a window is not the same as hanging up. Holding the store
 * here keeps the session across navigation and Back.
 */
class TyuApplication : Application(), ViewModelStoreOwner {
    override val viewModelStore = ViewModelStore()
}
