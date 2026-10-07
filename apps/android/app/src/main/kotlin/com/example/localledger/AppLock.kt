package com.example.localledger

import android.app.Activity
import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

object AppLockPolicy {
    fun canEnable(secure:Boolean,authenticated:Boolean)=secure && authenticated
}
object AppLock {
    const val FILE="application_lock"
    @Volatile var unlocked=false
    fun enabled(context:Context)=context.getSharedPreferences(FILE,0).getBoolean("enabled",false)
    fun save(context:Context,enabled:Boolean) {check(context.getSharedPreferences(FILE,0).edit().putBoolean("enabled",enabled).commit());unlocked=false}
}

/** Opaque gate: no ledger UI is drawn until system credential authentication succeeds. */
class LockActivity:Activity() {
    private var requesting=false
    override fun onCreate(state:Bundle?) {
        super.onCreate(state)
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        val layout=LinearLayout(this).apply {orientation=LinearLayout.VERTICAL;setPadding(32,48,32,32)}
        layout.addView(TextView(this).apply {text="账本已锁定，请使用设备 PIN、图案或密码解锁。";textSize=18f})
        layout.addView(Button(this).apply {text="使用系统凭证解锁";setOnClickListener {authenticate()}})
        layout.addView(Button(this).apply {text="退出";setOnClickListener {finishAffinity()}})
        setContentView(layout)
        if(!AppLock.enabled(this)) {finish();return}
        if(state==null) authenticate()
    }
    private fun authenticate() {
        if(requesting)return
        val manager=getSystemService(KeyguardManager::class.java)
        val intent=manager.createConfirmDeviceCredentialIntent("解锁 MeowM","验证设备凭证")
        if(intent!=null) {requesting=true;startActivityForResult(intent,40)}
    }
    @Deprecated("Framework result API")
    override fun onActivityResult(request:Int,result:Int,data:Intent?) {
        super.onActivityResult(request,result,data);requesting=false
        if(request==40 && result==RESULT_OK) {AppLock.unlocked=true;finish()}
    }
    @Deprecated("Back exits locked task")
    override fun onBackPressed() {finishAffinity()}
}
