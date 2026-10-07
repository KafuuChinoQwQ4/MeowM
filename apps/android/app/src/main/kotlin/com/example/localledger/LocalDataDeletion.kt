package com.example.localledger
import android.content.Context
import java.io.File

/** SQLite deletion is atomic; Keystore erasure follows and is safely retryable. */
object LocalDataDeletion {
    fun prepare(context:Context) {
        check(context.getSharedPreferences(CapturePreferences.FILE,0).edit().clear().commit())
        RustBridge.setSourceEnabled(CapturePreferences.WECHAT,false)
        RustBridge.setSourceEnabled(CapturePreferences.ALIPAY,false)
    }
    fun finish(context:Context) {
        AppLock.save(context,false)
        val store=ProviderCredentialStore(context)
        for(provider in listOf("wechat","alipay","bank")) check(store.delete(provider))
        for(name in listOf("ledger-backup-input","ledger-csv-input",".ledger-backup-input.part",".ledger-csv-input.part")) {
            val file=File(context.cacheDir,name)
            check(!file.exists() || file.delete())
        }
    }
}
