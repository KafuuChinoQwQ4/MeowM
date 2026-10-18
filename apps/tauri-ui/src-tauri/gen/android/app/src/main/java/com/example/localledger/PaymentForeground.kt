package com.example.localledger
import android.app.AppOpsManager
import android.app.usage.UsageEvents
import android.app.usage.UsageStatsManager
import android.content.Context
import android.os.Build
import android.os.Process

/** Package identity comes from OS events, never from OCR text. */
class PaymentForeground(private val context: Context) {
    private var current: String? = null
    private var checkedAt = System.currentTimeMillis() - 10_000
    fun currentPackage(): String? {
        if (!granted(context)) { current = null; return null }
        val now = System.currentTimeMillis()
        val manager = context.getSystemService(Context.USAGE_STATS_SERVICE) as UsageStatsManager
        val events = manager.queryEvents(checkedAt, now)
        checkedAt = now
        val event = UsageEvents.Event()
        while (events.hasNextEvent()) {
            events.getNextEvent(event)
            when (event.eventType) {
                UsageEvents.Event.MOVE_TO_FOREGROUND -> current = event.packageName
                UsageEvents.Event.MOVE_TO_BACKGROUND -> if (current == event.packageName) current = null
            }
        }
        return current?.takeIf { it == "com.tencent.mm" || it == "com.eg.android.AlipayGphone" }
    }
    companion object {
        fun granted(context: Context): Boolean {
            val ops = context.getSystemService(Context.APP_OPS_SERVICE) as AppOpsManager
            return (if (Build.VERSION.SDK_INT >= 29) ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), context.packageName)
                else ops.checkOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), context.packageName)) == AppOpsManager.MODE_ALLOWED
        }
    }
}
