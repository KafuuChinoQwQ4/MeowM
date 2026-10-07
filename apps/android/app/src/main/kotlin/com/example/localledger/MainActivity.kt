package com.example.localledger

import android.app.Activity
import android.content.Intent
import android.content.res.ColorStateList
import android.graphics.Color
import android.os.Bundle
import android.provider.Settings
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Switch
import android.widget.TextView

class MainActivity : Activity() {
    private lateinit var permissionStatus: TextView
    private lateinit var wechatSwitch: Switch
    private lateinit var alipaySwitch: Switch
    private lateinit var lockSwitch: Switch

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        RustBridge.initialize("${filesDir}/ledger.sqlite3")
        val preferences = getSharedPreferences(CapturePreferences.FILE, MODE_PRIVATE)
        val canvas = Color.rgb(255, 248, 252)
        val ink = Color.rgb(37, 38, 58)
        val muted = Color.rgb(119, 120, 142)
        val accent = Color.rgb(82, 123, 200)
        val pad = (20 * resources.displayMetrics.density).toInt()
        val layout = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad, pad, pad)
            setBackgroundColor(canvas)
        }
        layout.addView(TextView(this).apply {
            text = "MeowM"
            textSize = 28f
            setTextColor(accent)
            setTypeface(null, android.graphics.Typeface.BOLD)
        })
        layout.addView(TextView(this).apply {
            text = "通知与隐私"
            textSize = 18f
            setTextColor(ink)
            setPadding(0, pad, 0, pad / 2)
        })
        layout.addView(TextView(this).apply {
            text = getString(R.string.notification_permission_explanation)
            textSize = 14f
            setTextColor(muted)
        })
        permissionStatus = TextView(this).apply {
            textSize = 15f
            setTextColor(accent)
            setPadding(0, pad, 0, pad / 2)
        }
        layout.addView(permissionStatus)
        layout.addView(Button(this).apply {
            text = getString(R.string.open_notification_settings)
            setTextColor(Color.WHITE)
            backgroundTintList = ColorStateList.valueOf(accent)
            setOnClickListener { startActivity(Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)) }
        })
        layout.addView(TextView(this).apply {
            text = "采集来源"
            textSize = 18f
            setTextColor(ink)
            setPadding(0, pad, 0, pad / 2)
        })
        wechatSwitch = Switch(this).apply {
            text = getString(R.string.capture_wechat)
            setTextColor(ink)
            thumbTintList = ColorStateList.valueOf(accent)
            isChecked = preferences.getBoolean(CapturePreferences.WECHAT_ENABLED, CapturePreferences.DEFAULT_ENABLED)
            setOnCheckedChangeListener { _, enabled -> saveSource(preferences, CapturePreferences.WECHAT, CapturePreferences.WECHAT_ENABLED, enabled) }
        }
        layout.addView(wechatSwitch)
        alipaySwitch = Switch(this).apply {
            text = getString(R.string.capture_alipay)
            setTextColor(ink)
            thumbTintList = ColorStateList.valueOf(accent)
            isChecked = preferences.getBoolean(CapturePreferences.ALIPAY_ENABLED, CapturePreferences.DEFAULT_ENABLED)
            setOnCheckedChangeListener { _, enabled -> saveSource(preferences, CapturePreferences.ALIPAY, CapturePreferences.ALIPAY_ENABLED, enabled) }
        }
        layout.addView(alipaySwitch)
        layout.addView(TextView(this).apply {
            text = "设备安全"
            textSize = 18f
            setTextColor(ink)
            setPadding(0, pad, 0, pad / 2)
        })
        lockSwitch=Switch(this).apply {
            text="应用锁（设备 PIN / 图案 / 密码）"
            setTextColor(ink)
            thumbTintList = ColorStateList.valueOf(accent)
            isChecked=AppLock.enabled(this@MainActivity)
            setOnCheckedChangeListener { _, enabled ->
                if(enabled==AppLock.enabled(this@MainActivity)) return@setOnCheckedChangeListener
                if(!enabled) {AppLock.save(this@MainActivity,false);return@setOnCheckedChangeListener}
                val manager=getSystemService(android.app.KeyguardManager::class.java)
                val intent=manager.createConfirmDeviceCredentialIntent("启用应用锁","先验证您的设备凭证")
                if(intent==null) {isChecked=false;android.widget.Toast.makeText(this@MainActivity,"请先在系统设置中设置锁屏凭证",android.widget.Toast.LENGTH_LONG).show()}
                else startActivityForResult(intent,41)
            }
        }
        layout.addView(lockSwitch)
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_SECURE)
        window.statusBarColor = canvas
        window.navigationBarColor = canvas
        window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
        setContentView(ScrollView(this).apply { addView(layout) })
        RustBridge.setSourceEnabled(CapturePreferences.WECHAT, wechatSwitch.isChecked)
        RustBridge.setSourceEnabled(CapturePreferences.ALIPAY, alipaySwitch.isChecked)
    }

    override fun onResume() {
        super.onResume()
        val locked=AppLock.enabled(this) && !AppLock.unlocked
        window.decorView.visibility=if(locked) android.view.View.INVISIBLE else android.view.View.VISIBLE
        if(locked) startActivity(Intent(this,LockActivity::class.java))
        val enabled = Settings.Secure.getString(contentResolver, "enabled_notification_listeners")
            ?.split(':')
            ?.any { it.startsWith("$packageName/") } == true
        permissionStatus.text = getString(if (enabled) R.string.notification_access_enabled else R.string.notification_access_disabled)
    }

    @Deprecated("Framework result API")
    override fun onActivityResult(request:Int,result:Int,data:Intent?) {
        super.onActivityResult(request,result,data)
        if(request==41) {
            val accepted=AppLockPolicy.canEnable(getSystemService(android.app.KeyguardManager::class.java).isDeviceSecure,result==RESULT_OK)
            if(accepted) {AppLock.save(this,true);AppLock.unlocked=true}
            lockSwitch.isChecked=AppLock.enabled(this)
        }
    }
    override fun onPause() {if(AppLock.enabled(this)) window.decorView.visibility=android.view.View.INVISIBLE;super.onPause()}
    override fun onStop() {super.onStop();AppLock.unlocked=false}
    private fun saveSource(preferences: android.content.SharedPreferences, packageName: String, key: String, enabled: Boolean) {
        preferences.edit().putBoolean(key, enabled).apply()
        RustBridge.setSourceEnabled(packageName, enabled)
    }
}
