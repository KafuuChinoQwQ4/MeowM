package com.example.localledger
import org.junit.Assert.*
import org.junit.Test
class AppLockPolicyTest {
 @Test fun enablingRequiresDeviceCredentialAndSuccessfulAuthentication() {
    assertFalse(AppLockPolicy.canEnable(false,true))
    assertFalse(AppLockPolicy.canEnable(true,false))
    assertTrue(AppLockPolicy.canEnable(true,true))
 }
}
