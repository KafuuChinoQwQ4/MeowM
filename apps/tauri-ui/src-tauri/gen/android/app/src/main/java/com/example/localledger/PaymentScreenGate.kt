package com.example.localledger

/** Stabilize OCR before emitting once per continuous success page. No text is retained. */
class PaymentScreenGate {
    private var fingerprint: Int? = null
    private var stable = 0
    private var sent = false
    private var empty = 0
    private var page = 0L
    private var activePage: Long? = null
    private var source: String? = null
    fun observe(packageName: String?, success: Boolean, hash: Int): Long? {
        if (source != packageName) { reset(); source = packageName }
        if (packageName == null || !success) {
            empty++
            if (empty >= 2) reset()
            return null
        }
        empty = 0
        if (sent) return null
        stable = if (fingerprint == hash) stable + 1 else 1
        fingerprint = hash
        if (stable < 2) return null
        sent = true
        if (activePage == null) { page++; activePage = page }
        return activePage
    }
    fun retry() { sent = false; stable = 0 }
    private fun reset() { activePage = null; fingerprint = null; stable = 0; sent = false; empty = 0 }
}
