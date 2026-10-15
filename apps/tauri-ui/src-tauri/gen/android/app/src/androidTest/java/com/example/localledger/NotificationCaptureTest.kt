package com.example.localledger

import android.app.Notification
import android.content.Context
import android.content.ContextWrapper
import android.database.sqlite.SQLiteDatabase
import android.service.notification.StatusBarNotification
import android.os.Process
import android.os.UserHandle
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class NotificationCaptureTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext
    private fun event() = JSONObject().put("package_name", "com.tencent.mm").put("title", "微信支付")
        .put("body", "向咖啡店付款12元，private-body").put("notification_key", "private-key")
        .put("posted_at_ms", 1760000000000L)
    private fun seed(path: String) {
        assertFalse(RustBridge.nativeEnabled(path, "com.tencent.mm"))
        SQLiteDatabase.openDatabase(path, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.execSQL("INSERT INTO accounts VALUES ('00000000-0000-4000-8000-000000000001','测试现金','asset','CNY',2,0,0)")
            db.execSQL("INSERT INTO notification_settings(package_name,enabled,account_id) VALUES ('com.tencent.mm',1,'00000000-0000-4000-8000-000000000001') ON CONFLICT(package_name) DO UPDATE SET enabled=1,account_id=excluded.account_id")
        }
    }
    @Test
    fun jniIsBoundedDefaultOffAndDoesNotEchoContent() {
        val dir = File(context.cacheDir, UUID.randomUUID().toString()).apply { mkdirs() }
        try {
            val path = File(dir, "ledger.sqlite3").absolutePath
            assertFalse(RustBridge.nativeEnabled(path, "com.tencent.mm"))
            val result = RustBridge.nativeCapture(path, event().toString())
            assertEquals("ignored", JSONObject(result).getString("code"))
            assertFalse(result.contains("private-body"))
            assertEquals("error", JSONObject(RustBridge.nativeCapture(path, "invalid")).getString("code"))
            assertEquals("error", JSONObject(RustBridge.nativeCapture(path, "x".repeat(65537))).getString("code"))
            seed(path)
            val pending = JSONObject(RustBridge.nativeCapture(path, event().toString()))
            assertEquals("pending", pending.getString("code"))
            val duplicate = JSONObject(RustBridge.nativeCapture(path, event().toString()))
            assertEquals("duplicate", duplicate.getString("code"))
            assertEquals(pending.getString("transaction_id"), duplicate.getString("transaction_id"))
            SQLiteDatabase.openDatabase(path, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
                db.rawQuery("SELECT status FROM transactions", null).use { c ->
                    assertTrue(c.moveToFirst()); assertEquals("pending", c.getString(0)); assertFalse(c.moveToNext())
                }
                db.execSQL("UPDATE notification_settings SET enabled=0")
            }
            assertFalse(RustBridge.nativeEnabled(path, "com.tencent.mm"))
            assertEquals("ignored", JSONObject(RustBridge.nativeCapture(path, event().put("posted_at_ms", 1760000000001L).toString())).getString("code"))
            assertFalse(File(path).readBytes().toString(Charsets.ISO_8859_1).contains("private-body"))
        } finally { dir.deleteRecursively() }
    }
    @Test
    fun androidCallbackFiltersAndSurvivesServiceRecreation() {
        val dir = File(context.cacheDir, UUID.randomUUID().toString()).apply { mkdirs() }
        val path = File(dir, "ledger.sqlite3").absolutePath
        val isolated = object : ContextWrapper(context) { override fun getDataDir(): File = dir }
        val attach = ContextWrapper::class.java.getDeclaredMethod("attachBaseContext", Context::class.java).apply { isAccessible = true }
        fun service() = NotificationCaptureService().also { attach.invoke(it, isolated) }
        fun notification(pkg: String, flags: Int = 0): StatusBarNotification {
            val n = Notification.Builder(context, "test").setContentTitle("微信支付")
                .setContentText("向咖啡店付款12元，private-body").build().apply { this.flags = flags }
            return StatusBarNotification(pkg, pkg, 42, "fixture", Process.myUid(), 0, 0, n, UserHandle.getUserHandleForUid(Process.myUid()), 1760000000000L)
        }
        fun count(): Int = SQLiteDatabase.openDatabase(path, null, SQLiteDatabase.OPEN_READONLY).use { db ->
            db.rawQuery("SELECT COUNT(*) FROM transactions", null).use { c -> c.moveToFirst(); c.getInt(0) }
        }
        var listener: NotificationCaptureService? = null
        try {
            seed(path)
            listener = service()
            listener.onNotificationPosted(notification("unsupported.app"))
            listener.onNotificationPosted(notification("com.tencent.mm", Notification.FLAG_GROUP_SUMMARY))
            assertEquals(0, count())
            listener.onNotificationPosted(notification("com.tencent.mm"))
            val deadline = System.currentTimeMillis() + 5000
            while (count() == 0 && System.currentTimeMillis() < deadline) Thread.sleep(30)
            assertEquals(1, count())
            listener.onDestroy()
            listener = service()
            listener.onNotificationPosted(notification("com.tencent.mm"))
            val duplicateDeadline = System.currentTimeMillis() + 5000
            var duplicate = false
            while (!duplicate && System.currentTimeMillis() < duplicateDeadline) {
                SQLiteDatabase.openDatabase(path, null, SQLiteDatabase.OPEN_READONLY).use { db ->
                    db.rawQuery("SELECT code FROM notification_status", null).use { c -> duplicate = c.moveToFirst() && c.getString(0) == "duplicate" }
                }
                if (!duplicate) Thread.sleep(30)
            }
            assertTrue(duplicate)
            assertEquals(1, count())
        } finally { listener?.onDestroy(); dir.deleteRecursively() }
    }
    @Test
    fun serviceIsProtectedAndSharesTauriDatabaseDirectory() {
        assertEquals(File(context.dataDir, "ledger.sqlite3").absolutePath, RustBridge.databasePath(context))
        val info = context.packageManager.getServiceInfo(NotificationAccess.component(context), 0)
        assertEquals("android.permission.BIND_NOTIFICATION_LISTENER_SERVICE", info.permission)
        assertTrue(info.exported)
    }
}
