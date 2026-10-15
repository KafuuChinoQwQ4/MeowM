package com.example.localledger

import android.app.Notification
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import org.json.JSONObject
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit

/** Android extracts only transport fields. It never logs or saves notification text. */
class NotificationCaptureService : NotificationListenerService() {
    companion object { @Volatile var connected = false; private set }
    private val worker = ThreadPoolExecutor(1, 1, 30, TimeUnit.SECONDS,
        ArrayBlockingQueue(64), ThreadPoolExecutor.DiscardPolicy())
    override fun onListenerConnected() {
        connected = true
        // Re-delivery after reconnect is safe: Rust persists notification receipts.
        try { activeNotifications?.forEach { onNotificationPosted(it) } }
        catch (_: Exception) { /* Access can be revoked during reconnection. */ }
    }
    override fun onListenerDisconnected() {
        connected = false
        try {
            if (NotificationAccess.granted(this)) requestRebind(NotificationAccess.component(this))
        } catch (_: Exception) { /* Background restrictions are reported through status(). */ }
    }
    override fun onDestroy() {
        connected = false
        worker.shutdownNow()
        super.onDestroy()
    }
    override fun onNotificationPosted(sbn: StatusBarNotification?) {
        if (sbn == null || sbn.packageName !in setOf("com.tencent.mm", "com.eg.android.AlipayGphone")) return
        if (sbn.notification.flags and Notification.FLAG_GROUP_SUMMARY != 0) return
        try {
            // Copy bounded fields before returning the system callback; SQLite/JNI run off-main.
            val extras = sbn.notification.extras
            val title = extras.getCharSequence(Notification.EXTRA_TITLE)?.toString()?.take(256) ?: ""
            val body = (extras.getCharSequence(Notification.EXTRA_BIG_TEXT)
                ?: extras.getCharSequence(Notification.EXTRA_TEXT))?.toString()?.take(4096) ?: ""
            val event = JSONObject().put("package_name", sbn.packageName).put("title", title)
                .put("body", body).put("notification_key", sbn.key).put("posted_at_ms", sbn.postTime).toString()
            val packageName = sbn.packageName
            worker.execute {
                try {
                    val path = RustBridge.databasePath(this)
                    if (RustBridge.nativeEnabled(path, packageName)) RustBridge.nativeCapture(path, event)
                } catch (_: Exception) { /* Never emit private exception messages or notification text. */ }
                  catch (_: LinkageError) { /* A missing native library must not crash system callbacks. */ }
            }
        } catch (_: Exception) { /* Malformed extras must not block later notifications. */ }
    }
}
