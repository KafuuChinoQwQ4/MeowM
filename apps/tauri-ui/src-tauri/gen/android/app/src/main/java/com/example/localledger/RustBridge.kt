package com.example.localledger

import android.content.Context
import java.io.File

/** JNI only transports bounded events; the Rust core owns parsing/settings/storage. */
object RustBridge {
    init { System.loadLibrary("ledger_tauri_ui") }
    @JvmStatic external fun nativeCapture(path: String, event: String): String
    @JvmStatic external fun nativeEnabled(path: String, packageName: String): Boolean
    // Tauri's Android app_data_dir resolves to Context.dataDir, not filesDir.
    fun databasePath(context: Context): String = File(context.dataDir, "ledger.sqlite3").absolutePath
}
