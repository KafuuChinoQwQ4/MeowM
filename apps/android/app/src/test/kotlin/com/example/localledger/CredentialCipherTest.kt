package com.example.localledger
import org.junit.Assert.*
import org.junit.Test
import javax.crypto.KeyGenerator

class CredentialCipherTest {
    @Test fun authenticatedEnvelopeRejectsTamperAndProviderSwap() {
        val key=KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
        val token="synthetic-offline-credential".toByteArray()
        val first=CredentialCipher.encrypt(key,"wechat",token)
        val second=CredentialCipher.encrypt(key,"wechat",token)
        assertFalse(first.contentEquals(second))
        assertFalse(String(first).contains(String(token)))
        assertArrayEquals(token,CredentialCipher.decrypt(key,"wechat",first))
        first[first.lastIndex]=(first.last().toInt() xor 1).toByte()
        for ((provider,bytes) in listOf("wechat" to first,"alipay" to second,"wechat" to byteArrayOf(1))) {
            try { CredentialCipher.decrypt(key,provider,bytes);fail("Invalid credential accepted") } catch (_:Exception) { }
        }
    }
}
