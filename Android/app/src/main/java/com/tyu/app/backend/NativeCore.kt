package com.tyu.app.backend

/** Small JNI control boundary. QUIC, identity and trust all live in the same Rust Core as Desktop. */
object NativeCore {
    init { System.loadLibrary("tyu_ffi") }
    external fun initialize(directory: String, name: String): Long
    external fun start(handle: Long): Int
    external fun snapshot(handle: Long): String
    external fun command(handle: Long, kind: String, value: String): String
    external fun poll(handle: Long): String
    external fun shutdown(handle: Long): Int
}
