package com.example.localledger

import android.os.Bundle
import android.view.View
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    enableEdgeToEdge()
    // Initialize the keyring context before Tauri starts its Rust setup callback.
    RustBridge.nativeInitializeCredentialContext(applicationContext)
    super.onCreate(savedInstanceState)
    ExchangeRateWorkScheduler.ensureScheduled(this)
    ExchangeRateWorkScheduler.enqueueStartupSync(this)
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val keyboard = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, keyboard.bottom))
      WindowInsetsCompat.CONSUMED
    }
    ViewCompat.requestApplyInsets(content)
  }

}
