package com.example.localledger

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.security.KeyStore
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

/** Android Keystore holds non-exportable AES keys; only ciphertext lives in noBackupFilesDir.
 * No preferences, SQLite, logs, token Strings, network or implicit authorization here.
 * Exceptions are intentionally caught/sanitized by the JNI port, never logged.
 */
class ProviderCredentialStore(context: Context) {
    private val directory=File(context.applicationContext.noBackupFilesDir,"provider-credentials")
    private fun slot(provider: String): AtomicFile {
        require(provider in setOf("wechat","alipay","bank"))
        check(directory.isDirectory || directory.mkdirs())
        return AtomicFile(File(directory,"$provider.bin"))
    }
    private fun keyStore()=KeyStore.getInstance("AndroidKeyStore").apply {load(null)}
    private fun key(provider: String, create: Boolean): SecretKey? {
        val alias="meowm.provider.$provider"
        val old=keyStore().getKey(alias,null)
        if(old != null) return old as SecretKey
        if(!create) return null
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES,"AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(alias,KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256).setRandomizedEncryptionRequired(true).build())
        }.generateKey()
    }
    fun save(provider: String, token: ByteArray): Boolean = synchronized(lock) {
        val file=slot(provider)
        val encrypted=CredentialCipher.encrypt(key(provider,true)!!,provider,token)
        val stream=file.startWrite()
        try {stream.write(encrypted);file.finishWrite(stream)}
        catch(e:Exception){file.failWrite(stream);throw e}
        true
    }
    fun load(provider: String): ByteArray? = synchronized(lock) {
        val file=slot(provider)
        val envelope=try {file.openRead().use {input->
            val buffer=ByteArray(16414)
            var count=0
            while(count<buffer.size) {
                val n=input.read(buffer,count,buffer.size-count)
                if(n<0) break
                count+=n
            }
            require(count<=16413)
            buffer.copyOf(count)
        }} catch (_:java.io.FileNotFoundException) {return@synchronized null}
        val existing=key(provider,false) ?: throw java.io.IOException("Credential key unavailable")
        CredentialCipher.decrypt(existing,provider,envelope)
    }
    fun delete(provider: String): Boolean = synchronized(lock) {
        val file=slot(provider)
        // Delete the decryption key first; failure must not report successful revocation.
        keyStore().deleteEntry("meowm.provider.$provider")
        file.delete()
        check(!file.baseFile.exists())
        true
    }
    companion object {private val lock=Any()}
}
