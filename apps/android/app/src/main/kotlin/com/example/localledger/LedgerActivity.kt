package com.example.localledger

import android.app.NativeActivity
import android.content.Intent
import android.os.Bundle
import java.io.File

/** Android handles SAF streams only. Passwords and ledger mutation stay in Rust. */
class LedgerActivity : NativeActivity() {
    private val documentResult=java.util.concurrent.atomic.AtomicReference("")
    private var pendingExport: ByteArray?=null
    @Volatile private var pendingRequest=0
    override fun onCreate(savedInstanceState: Bundle?) {
        RustBridge.initialize("${filesDir}/ledger.sqlite3")
        super.onCreate(savedInstanceState)
        // Ciphertext/CSV staging is private, transient, and not part of a backup.
        File(cacheDir,"ledger-backup-input").delete()
        File(cacheDir,"ledger-csv-input").delete()
    }
    override fun onResume() {
        super.onResume()
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE)
        val locked=AppLock.enabled(this) && !AppLock.unlocked
        window.decorView.visibility=if(locked) android.view.View.INVISIBLE else android.view.View.VISIBLE
        if(locked) startActivity(Intent(this,LockActivity::class.java))
    }
    override fun onPause() {if(AppLock.enabled(this)) window.decorView.visibility=android.view.View.INVISIBLE;super.onPause()}
    override fun onStop() {super.onStop();AppLock.unlocked=false}
    @Suppress("unused") fun deleteLocalData(finish:Boolean):Boolean = runCatching {
        if(pendingRequest!=0) return@runCatching false
        if(finish) {LocalDataDeletion.finish(this);documentResult.set("");pendingExport=null}
        else LocalDataDeletion.prepare(this)
        true
    }.getOrDefault(false)
    @Suppress("unused") fun openNotificationSettings() {
        runOnUiThread { startActivity(Intent(this, MainActivity::class.java)) }
    }
    @Suppress("unused") fun chooseDocument(kind:Int, bytes:ByteArray) {
        runOnUiThread {
            if (pendingRequest!=0) { documentResult.set("error:已有文件操作正在进行"); return@runOnUiThread }
            pendingRequest=kind
            if(kind==1) pendingExport=bytes
            val intent=Intent(if(kind==1) Intent.ACTION_CREATE_DOCUMENT else Intent.ACTION_OPEN_DOCUMENT).apply {
                type="*/*"; addCategory(Intent.CATEGORY_OPENABLE)
                if(kind==1) { type="application/octet-stream"; putExtra(Intent.EXTRA_TITLE,"local-ledger.meowm") }
            }
            try { startActivityForResult(intent,kind) }
            catch (_: Exception) { pendingRequest=0; pendingExport=null; documentResult.set("error:无法打开系统文件选择器") }
        }
    }
    @Suppress("unused") @Synchronized fun takeDocumentResult():String {
        return documentResult.getAndSet("")
    }
    @Deprecated("NativeActivity uses the framework activity result API")
    override fun onActivityResult(requestCode:Int,resultCode:Int,data:Intent?) {
        super.onActivityResult(requestCode,resultCode,data)
        if (requestCode !in 1..3) return
        if (resultCode!=RESULT_OK || data?.data==null) {
            pendingRequest=0;pendingExport=null;documentResult.set("cancel:已取消文件操作");return
        }
        val uri=data.data!!
        val ciphertext=pendingExport;pendingExport=null
        // Slow document providers must not block the Activity main thread.
        Thread {
            try {
                when(requestCode) {
                    1 -> {
                        val bytes=ciphertext ?: throw java.io.IOException()
                        contentResolver.openOutputStream(uri,"wt")?.use { it.write(bytes);it.flush() } ?: throw java.io.IOException()
                        documentResult.set("export:已导出加密备份")
                    }
                    2,3 -> {
                        val file=File(cacheDir,if(requestCode==2) "ledger-backup-input" else "ledger-csv-input")
                        DocumentTransfer.importUri(contentResolver,uri,file,if(requestCode==2) DocumentTransfer.MAX_BACKUP_BYTES+128 else DocumentTransfer.MAX_CSV_BYTES)
                        documentResult.set((if(requestCode==2) "backup:" else "csv:")+file.absolutePath)
                    }
                }
            } catch (_: Exception) {
                if(requestCode==1) runCatching { android.provider.DocumentsContract.deleteDocument(contentResolver,uri) }
                documentResult.set("error:文件读写失败或文件过大，请重新选择")
            }
            finally { pendingRequest=0 }
        }.start()
    }
}
