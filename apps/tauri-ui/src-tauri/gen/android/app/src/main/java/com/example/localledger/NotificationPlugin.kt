package com.example.localledger

import android.app.Activity
import android.content.Intent
import android.provider.Settings
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@TauriPlugin
class NotificationPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun status(invoke: Invoke) {
        try {
            invoke.resolve(JSObject().put("android", true)
                .put("granted", NotificationAccess.granted(activity))
                .put("connected", NotificationCaptureService.connected))
        } catch (_: Exception) { invoke.reject("无法读取通知访问状态") }
    }
    @Command
    fun openSettings(invoke: Invoke) {
        try {
            activity.startActivity(Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS))
            invoke.resolve(JSObject())
        } catch (_: Exception) { invoke.reject("无法打开系统通知访问设置") }
    }
}
