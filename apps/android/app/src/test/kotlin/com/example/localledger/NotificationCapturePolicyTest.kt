package com.example.localledger

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NotificationCapturePolicyTest {
    @Test fun notificationSourcesAreOffUntilTheUserOptsIn() {
        assertFalse(CapturePreferences.DEFAULT_ENABLED)
        assertFalse(shouldForwardNotification(CapturePreferences.WECHAT, CapturePreferences.DEFAULT_ENABLED, CapturePreferences.DEFAULT_ENABLED))
        assertFalse(shouldForwardNotification(CapturePreferences.ALIPAY, CapturePreferences.DEFAULT_ENABLED, CapturePreferences.DEFAULT_ENABLED))
    }

    @Test fun disabledWechatIsNotForwardedWhileAlipayRemainsEnabled() {
        assertFalse(shouldForwardNotification(CapturePreferences.WECHAT, false, true))
        assertTrue(shouldForwardNotification(CapturePreferences.ALIPAY, false, true))
    }

    @Test fun unrelatedPackagesAreNeverForwarded() {
        assertFalse(shouldForwardNotification("com.example.other", true, true))
    }
}
