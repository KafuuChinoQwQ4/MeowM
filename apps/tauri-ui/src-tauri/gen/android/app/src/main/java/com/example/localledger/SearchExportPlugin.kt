package com.example.localledger

import android.app.Activity
import android.content.Intent
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.nio.charset.StandardCharsets

@TauriPlugin
class SearchExportPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun saveCsv(invoke: Invoke) {
        val csv = invoke.getArgs().getString("csv")
        if (csv.isEmpty() || csv.toByteArray(StandardCharsets.UTF_8).size > 16 * 1024 * 1024) {
            invoke.reject("导出内容为空或超过 16 MB")
            return
        }
        try {
            val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = "text/csv"
                putExtra(Intent.EXTRA_TITLE, "meowm-search.csv")
            }
            startActivityForResult(invoke, intent, "saveResult")
        } catch (_: Exception) {
            invoke.reject("无法打开系统保存位置")
        }
    }

    @ActivityCallback
    fun saveResult(invoke: Invoke, result: ActivityResult) {
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null) {
            invoke.reject("已取消导出")
            return
        }
        try {
            val bytes = invoke.getArgs().getString("csv").toByteArray(StandardCharsets.UTF_8)
            activity.contentResolver.openOutputStream(uri, "wt")?.use { it.write(bytes) }
                ?: throw IllegalStateException("无法写入所选位置")
            invoke.resolve(JSObject().put("saved", true))
        } catch (_: Exception) {
            invoke.reject("无法写入所选位置")
        }
    }
}
