package com.example.localledger

import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification

/** Android extracts only the five transient fields accepted by the Rust boundary. */
class NotificationCaptureService : NotificationListenerService() {
    override fun onCreate() {
        super.onCreate()
        initializeBridge()
    }

    override fun onListenerConnected() {
        super.onListenerConnected()
        initializeBridge()
    }

    override fun onNotificationPosted(sbn: StatusBarNotification) {
        runCatching { forwardIfEnabled(sbn) }
    }

    private fun initializeBridge() {
        runCatching {
            RustBridge.initialize("${applicationContext.filesDir}/ledger.sqlite3")
            val preferences = getSharedPreferences(CapturePreferences.FILE, MODE_PRIVATE)
            RustBridge.setSourceEnabled(CapturePreferences.WECHAT, preferences.getBoolean(CapturePreferences.WECHAT_ENABLED, CapturePreferences.DEFAULT_ENABLED))
            RustBridge.setSourceEnabled(CapturePreferences.ALIPAY, preferences.getBoolean(CapturePreferences.ALIPAY_ENABLED, CapturePreferences.DEFAULT_ENABLED))
        }
    }

    private fun forwardIfEnabled(sbn: StatusBarNotification) {
        val preferences = getSharedPreferences(CapturePreferences.FILE, MODE_PRIVATE)
        if (!shouldForwardNotification(
                sbn.packageName,
                preferences.getBoolean(CapturePreferences.WECHAT_ENABLED, CapturePreferences.DEFAULT_ENABLED),
                preferences.getBoolean(CapturePreferences.ALIPAY_ENABLED, CapturePreferences.DEFAULT_ENABLED),
            )) return
        val extras = sbn.notification.extras
        val title = extras.getCharSequence("android.title")?.toString().orEmpty()
        val body = extras.getCharSequence("android.text")?.toString().orEmpty()
        RustBridge.onPaymentNotification(sbn.packageName, title, body, sbn.key, sbn.postTime)
    }
}

internal object CapturePreferences {
    const val DEFAULT_ENABLED = false
    const val FILE = "notification_capture"
    const val WECHAT = "com.tencent.mm"
    const val ALIPAY = "com.eg.android.AlipayGphone"
    const val WECHAT_ENABLED = "wechat_enabled"
    const val ALIPAY_ENABLED = "alipay_enabled"
}

internal fun shouldForwardNotification(packageName: String, wechatEnabled: Boolean, alipayEnabled: Boolean): Boolean = when (packageName) {
    CapturePreferences.WECHAT -> wechatEnabled
    CapturePreferences.ALIPAY -> alipayEnabled
    else -> false
}
