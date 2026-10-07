package com.example.localledger
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** A synthetic input through the production JNI boundary, leaving Pending for UI review. */
@RunWith(AndroidJUnit4::class)
class OfflineWorkflowInstrumentedTest {
 @Test fun seedPendingForOfflineUiReview() {
  val context=InstrumentationRegistry.getInstrumentation().targetContext
  assertTrue(RustBridge.initialize(File(context.filesDir,"ledger.sqlite3").path))
  RustBridge.setSourceEnabled(CapturePreferences.WECHAT,true)
  val result=RustBridge.onPaymentNotification(CapturePreferences.WECHAT,"微信支付","支付成功￥12.00 商户：Synthetic Cafe","task8-offline-synthetic",System.currentTimeMillis())
  assertEquals(RustBridge.PENDING,result)
  RustBridge.setSourceEnabled(CapturePreferences.WECHAT,false)
 }
}
