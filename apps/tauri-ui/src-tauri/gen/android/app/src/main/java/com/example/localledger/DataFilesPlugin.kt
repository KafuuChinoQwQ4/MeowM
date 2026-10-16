package com.example.localledger

import android.app.Activity
import android.content.Intent
import android.util.Base64
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@TauriPlugin
class DataFilesPlugin(private val activity: Activity) : Plugin(activity) {
    private val maxBytes = 32 * 1024 * 1024
    @Command
    fun saveFile(invoke: Invoke) {
        try {
            val args = invoke.getArgs()
            val encoded = args.getString("data")
            if (encoded.length > 45 * 1024 * 1024) throw IllegalArgumentException()
            val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = args.getString("mime")
                putExtra(Intent.EXTRA_TITLE, args.getString("name"))
            }
            startActivityForResult(invoke, intent, "saveResult")
        } catch (_: Exception) { invoke.reject("无法打开系统保存位置") }
    }
    @ActivityCallback
    fun saveResult(invoke: Invoke, result: ActivityResult) {
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null) {
            invoke.reject("已取消保存"); return
        }
        try {
            val bytes = Base64.decode(invoke.getArgs().getString("data"), Base64.NO_WRAP)
            if (bytes.size > maxBytes) throw IllegalArgumentException()
            activity.contentResolver.openOutputStream(uri, "wt")?.use { it.write(bytes) }
                ?: throw IllegalStateException()
            invoke.resolve(JSObject().put("saved", true))
        } catch (_: Exception) { invoke.reject("无法写入所选位置") }
    }
    @Command
    fun openBackup(invoke: Invoke) {
        try {
            val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = "*/*"
            }
            startActivityForResult(invoke, intent, "openResult")
        } catch (_: Exception) { invoke.reject("无法打开系统文件选择器") }
    }
    @ActivityCallback
    fun openResult(invoke: Invoke, result: ActivityResult) {
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null) {
            invoke.reject("已取消选择"); return
        }
        try {
            val bytes = activity.contentResolver.openInputStream(uri)?.use { input ->
                val out = java.io.ByteArrayOutputStream()
                val buffer = ByteArray(8192)
                var total = 0
                while (true) {
                    val n = input.read(buffer)
                    if (n < 0) break
                    total += n
                    if (total > maxBytes) throw IllegalArgumentException()
                    out.write(buffer, 0, n)
                }
                out.toByteArray()
            } ?: throw IllegalStateException()
            invoke.resolve(JSObject().put("data", Base64.encodeToString(bytes, Base64.NO_WRAP)))
        } catch (_: Exception) { invoke.reject("无法读取备份或文件超过 32 MB") }
    }
}
