package com.example.localledger

import javax.crypto.Cipher
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Version + random 96-bit nonce + authenticated ciphertext; never stores plaintext. */
internal object CredentialCipher {
    fun encrypt(key: SecretKey, provider: String, plaintext: ByteArray): ByteArray {
        require(plaintext.isNotEmpty() && plaintext.size <= 16384)
        val cipher=Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE,key)
        cipher.updateAAD("meowm.provider.v1.$provider".toByteArray(Charsets.UTF_8))
        require(cipher.iv.size == 12)
        return byteArrayOf(1)+cipher.iv+cipher.doFinal(plaintext)
    }
    fun decrypt(key: SecretKey, provider: String, envelope: ByteArray): ByteArray {
        require(envelope.size in 30..16413 && envelope[0] == 1.toByte())
        val cipher=Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE,key,GCMParameterSpec(128,envelope.copyOfRange(1,13)))
        cipher.updateAAD("meowm.provider.v1.$provider".toByteArray(Charsets.UTF_8))
        return cipher.doFinal(envelope,13,envelope.size-13)
    }
}
