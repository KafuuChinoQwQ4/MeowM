package com.example.localledger
import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.KeyStore

@RunWith(AndroidJUnit4::class)
class PrivacyInstrumentedTest {
 @Test fun localCleanupErasesAllCredentialKeysStagingAndDisablesCapture() {
    val context=InstrumentationRegistry.getInstrumentation().targetContext
    assertTrue(RustBridge.initialize(File(context.filesDir,"privacy-test.sqlite3").path))
    val store=ProviderCredentialStore(context)
    for(p in listOf("wechat","alipay","bank")) assertTrue(store.save(p,"synthetic-privacy-token".toByteArray()))
    File(context.cacheDir,"ledger-csv-input").writeText("synthetic bill")
    File(context.cacheDir,"ledger-backup-input").writeText("synthetic cipher")
    context.getSharedPreferences(CapturePreferences.FILE,0).edit().putBoolean(CapturePreferences.WECHAT_ENABLED,true).commit()
    RustBridge.setSourceEnabled(CapturePreferences.WECHAT,true)
    LocalDataDeletion.prepare(context)
    assertEquals(RustBridge.IGNORED,RustBridge.onPaymentNotification(CapturePreferences.WECHAT,"微信支付","付款成功，金额￥1.00","deletion-test",1791324000000))
    LocalDataDeletion.finish(context)
    for(p in listOf("wechat","alipay","bank")) {assertNull(store.load(p));assertFalse(KeyStore.getInstance("AndroidKeyStore").apply {load(null)}.containsAlias("meowm.provider.$p"))}
    assertFalse(File(context.cacheDir,"ledger-csv-input").exists());assertFalse(File(context.cacheDir,"ledger-backup-input").exists())
    assertFalse(context.getSharedPreferences(CapturePreferences.FILE,0).getBoolean(CapturePreferences.WECHAT_ENABLED,false))
    assertFalse(AppLock.enabled(context))
 }
 @Test fun privacyConfigurationDisablesSystemBackup() {
    val context=InstrumentationRegistry.getInstrumentation().targetContext
    assertEquals(0,context.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_ALLOW_BACKUP)
 }
 @Test fun lockedEntryHidesLedgerAndBackCannotUnlock() {
    val instrumentation=InstrumentationRegistry.getInstrumentation();val context=instrumentation.targetContext
    AppLock.save(context,true)
    val monitor=instrumentation.addMonitor(LockActivity::class.java.name,null,false)
    try {
        context.startActivity(Intent(context,LedgerActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        val gate=instrumentation.waitForMonitorWithTimeout(monitor,10000)
        assertNotNull(gate);assertFalse(AppLock.unlocked)
        instrumentation.runOnMainSync {gate.onBackPressed()}
        assertFalse(AppLock.unlocked)
    } finally {AppLock.save(context,false);instrumentation.removeMonitor(monitor)}
 }
}
