package com.example.localledger

import android.app.NotificationManager
import android.content.ComponentName
import android.content.Context
import android.os.Build
import android.provider.Settings

object NotificationAccess {
    fun component(context: Context) = ComponentName(context, NotificationCaptureService::class.java)
    fun granted(context: Context): Boolean {
        if (Build.VERSION.SDK_INT >= 27) {
            return context.getSystemService(NotificationManager::class.java)
                .isNotificationListenerAccessGranted(component(context))
        }
        val services = Settings.Secure.getString(context.contentResolver, "enabled_notification_listeners") ?: ""
        return services.split(':').any { ComponentName.unflattenFromString(it) == component(context) }
    }
}
