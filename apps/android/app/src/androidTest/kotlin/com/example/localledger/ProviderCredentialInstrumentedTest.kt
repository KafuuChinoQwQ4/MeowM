package com.example.localledger
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.KeyStore

@RunWith(AndroidJUnit4::class)
class ProviderCredentialInstrumentedTest {
 @Test fun keystoreAndJniRoundTripRestartTamperAndDelete() {
    val context=InstrumentationRegistry.getInstrumentation().targetContext
    val store=ProviderCredentialStore(context)
    val token="synthetic-offline-only-token".toByteArray()
    try {
      assertTrue(RustBridge.saveProviderCredential(store,"wechat",token))
      val restarted=ProviderCredentialStore(context)
      assertArrayEquals(token,RustBridge.loadProviderCredential(restarted,"wechat"))
      assertNull(RustBridge.loadProviderCredential(restarted,"alipay"))
      val file=File(context.noBackupFilesDir,"provider-credentials/wechat.bin")
      assertFalse(String(file.readBytes()).contains(String(token)))
      val key=KeyStore.getInstance("AndroidKeyStore").apply {load(null)}.getKey("meowm.provider.wechat",null)
      assertNull(key.encoded)
      val bytes=file.readBytes();bytes[bytes.lastIndex]=(bytes.last().toInt() xor 1).toByte();file.writeBytes(bytes)
      assertNull(RustBridge.loadProviderCredential(restarted,"wechat"))
      assertTrue(RustBridge.deleteProviderCredential(restarted,"wechat"))
      assertFalse(file.exists())
      assertFalse(KeyStore.getInstance("AndroidKeyStore").apply {load(null)}.containsAlias("meowm.provider.wechat"))
      assertFalse(RustBridge.saveProviderCredential(store,"../invalid",token))
    } finally { RustBridge.deleteProviderCredential(store,"wechat");token.fill(0) }
 }
}
