package com.example.localledger
import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.media.projection.MediaProjectionManager
import android.media.projection.MediaProjectionConfig
import android.os.Build
import android.provider.Settings
import androidx.activity.result.ActivityResult
import androidx.core.content.ContextCompat
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.PermissionState
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@TauriPlugin(permissions = [Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "notifications")])
class ScreenCapturePlugin(private val activity: Activity) : Plugin(activity) {
    @Command fun status(invoke: Invoke) {
        invoke.resolve(JSObject().put("android", true).put("running", PaymentCaptureService.running)
            .put("usage_granted", PaymentForeground.granted(activity)).put("code", PaymentCaptureService.code))
    }
    @Command fun openUsageSettings(invoke: Invoke) {
        try { activity.startActivity(Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS)); invoke.resolve(JSObject()) }
        catch (_: Exception) { invoke.reject("无法打开使用情况访问设置") }
    }
    @Command fun request(invoke: Invoke) {
        if (!PaymentForeground.granted(activity)) { invoke.reject("请先授予使用情况访问权限"); return }
        if (Build.VERSION.SDK_INT >= 33 && getPermissionState("notifications") != PermissionState.GRANTED) {
            requestPermissionForAlias("notifications", invoke, "notificationResult")
            return
        }
        requestProjection(invoke)
    }
    @PermissionCallback fun notificationResult(invoke: Invoke) { requestProjection(invoke) }
    private fun requestProjection(invoke: Invoke) {
        if (!PaymentCaptureService.notificationsAllowed(activity)) {
            try { activity.startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, activity.packageName)) }
            catch (_: Exception) { }
            invoke.reject("请先允许支付页面识别通知，再重新开始识别")
            return
        }
        try {
            val manager = activity.getSystemService(Context.MEDIA_PROJECTION_SERVICE) as MediaProjectionManager
            val intent = if (Build.VERSION.SDK_INT >= 34) manager.createScreenCaptureIntent(MediaProjectionConfig.createConfigForDefaultDisplay()) else manager.createScreenCaptureIntent()
            startActivityForResult(invoke, intent, "projectionResult")
        } catch (_: Exception) { invoke.reject("无法请求屏幕捕获授权") }
    }
    @ActivityCallback fun projectionResult(invoke: Invoke, result: ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK || result.data == null) { invoke.reject("屏幕授权已取消"); return }
        try {
            val intent = Intent(activity, PaymentCaptureService::class.java)
                .putExtra("result_code", result.resultCode).putExtra("result_data", result.data)
                .putExtra("session", invoke.getArgs().getString("session"))
            ContextCompat.startForegroundService(activity, intent)
            invoke.resolve(JSObject())
        } catch (_: Exception) { invoke.reject("无法启动屏幕识别") }
    }
    @Command fun stop(invoke: Invoke) {
        activity.stopService(Intent(activity, PaymentCaptureService::class.java))
        invoke.resolve(JSObject())
    }
}
