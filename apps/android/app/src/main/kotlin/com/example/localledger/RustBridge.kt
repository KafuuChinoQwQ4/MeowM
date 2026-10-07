package com.example.localledger

/** Narrow JNI boundary; notification fields cross as transient parser input only. */
object RustBridge {
    const val IGNORED = 0
    const val PENDING = 1
    const val CONFIRMED = 2
    const val ERROR = 3

    private val nativeAvailable: Boolean by lazy {
        runCatching { System.loadLibrary("ledger_android_bridge") }.isSuccess
    }

    fun initialize(databasePath: String): Boolean = runCatching {
        nativeAvailable && initializeNative(databasePath)
    }.getOrDefault(false)

    fun setSourceEnabled(packageName: String, enabled: Boolean) {
        runCatching { if (nativeAvailable) setSourceEnabledNative(packageName, enabled) }
    }

    fun onPaymentNotification(
        packageName: String,
        title: String,
        body: String,
        notificationIdentity: String,
        postedAtEpochMs: Long,
    ): Int = runCatching {
        if (nativeAvailable) onPaymentNotificationNative(packageName, title, body, notificationIdentity, postedAtEpochMs)
        else ERROR
    }.getOrDefault(ERROR)

    /** Future authorization boundary; this API does not connect to a provider. */
    fun saveProviderCredential(store: ProviderCredentialStore, provider: String, token: ByteArray): Boolean {
        val transient=token.copyOf()
        return try { nativeAvailable && saveProviderCredentialNative(store,provider,transient) }
        finally { transient.fill(0) }
    }
    fun loadProviderCredential(store: ProviderCredentialStore, provider: String): ByteArray? =
        if(nativeAvailable) loadProviderCredentialNative(store,provider) else null
    fun deleteProviderCredential(store: ProviderCredentialStore, provider: String): Boolean =
        nativeAvailable && deleteProviderCredentialNative(store,provider)
    private external fun saveProviderCredentialNative(store: ProviderCredentialStore, provider: String, token: ByteArray): Boolean
    private external fun loadProviderCredentialNative(store: ProviderCredentialStore, provider: String): ByteArray?
    private external fun deleteProviderCredentialNative(store: ProviderCredentialStore, provider: String): Boolean

    private external fun initializeNative(databasePath: String): Boolean
    private external fun setSourceEnabledNative(packageName: String, enabled: Boolean)
    private external fun onPaymentNotificationNative(
        packageName: String,
        title: String,
        body: String,
        notificationIdentity: String,
        postedAtEpochMs: Long,
    ): Int
}
