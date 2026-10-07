package com.example.localledger

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import android.database.sqlite.SQLiteDatabase
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NotificationBridgeInstrumentedTest {
    @Test fun jniReturnsPendingForRepeatCallbackWithoutCrashing() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val databaseFile = context.getDatabasePath("jni-test-ledger.sqlite3")
        databaseFile.delete()
        assertEquals(true, RustBridge.initialize(databaseFile.absolutePath))
        RustBridge.setSourceEnabled(CapturePreferences.WECHAT, false)

        val disabled = RustBridge.onPaymentNotification(
            CapturePreferences.WECHAT,
            "微信支付",
            "微信支付凭证\n支付金额：¥12.34",
            "disabled-source-key",
            1_791_361_800_000,
        )
        assertEquals(RustBridge.IGNORED, disabled)
        RustBridge.setSourceEnabled(CapturePreferences.WECHAT, true)

        val first = RustBridge.onPaymentNotification(
            CapturePreferences.WECHAT,
            "微信支付",
            "微信支付凭证\n支付金额：¥12.34\n商户名称：测试咖啡\n交易时间：2026-10-07 10:30:00",
            "instrumented-repeat-key",
            1_791_361_800_000,
        )
        val repeated = RustBridge.onPaymentNotification(
            CapturePreferences.WECHAT,
            "微信支付",
            "微信支付凭证\n支付金额：¥12.34\n商户名称：测试咖啡\n交易时间：2026-10-07 10:30:00",
            "instrumented-repeat-key",
            1_791_361_800_000,
        )

        assertEquals(RustBridge.PENDING, first)
        assertEquals(RustBridge.PENDING, repeated)
        SQLiteDatabase.openDatabase(databaseFile.absolutePath, null, SQLiteDatabase.OPEN_READONLY).use { database ->
            database.rawQuery("SELECT COUNT(*) FROM transactions", null).use { cursor ->
                cursor.moveToFirst()
                assertEquals(1, cursor.getInt(0))
            }
            database.rawQuery("SELECT COUNT(*) FROM source_evidence WHERE source_kind = 'notification'", null).use { cursor ->
                cursor.moveToFirst()
                assertEquals(1, cursor.getInt(0))
            }
        }
    }
}
