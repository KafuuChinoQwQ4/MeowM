package com.example.localledger
import org.junit.Assert.*
import org.junit.Test
class PaymentScreenGateTest {
    @Test fun failedDeliveryRetriesTheSameIdentityUntilThePageLeaves() {
        val gate = PaymentScreenGate()
        gate.observe("com.tencent.mm", true, 1)
        val first = gate.observe("com.tencent.mm", true, 1)
        gate.retry()
        assertNull(gate.observe("com.tencent.mm", true, 1))
        assertEquals(first, gate.observe("com.tencent.mm", true, 1))
        gate.observe(null, false, 0)
        gate.observe("com.tencent.mm", true, 1)
        assertNotEquals(first, gate.observe("com.tencent.mm", true, 1))
    }

    @Test fun emitsOnceUntilPageLeavesThenAllowsSameAmountAgain() {
        val gate = PaymentScreenGate()
        assertNull(gate.observe("com.tencent.mm", true, 1))
        val first = gate.observe("com.tencent.mm", true, 1)
        assertNotNull(first)
        repeat(50) { assertNull(gate.observe("com.tencent.mm", true, it)) }
        gate.observe("com.tencent.mm", false, 0)
        gate.observe("com.tencent.mm", false, 0)
        assertNull(gate.observe("com.tencent.mm", true, 1))
        assertNotEquals(first, gate.observe("com.tencent.mm", true, 1))
    }
    @Test fun changingSourceAndUnstableOcrNeverReusePreviousCandidate() {
        val gate = PaymentScreenGate()
        assertNull(gate.observe("com.tencent.mm", true, 1))
        assertNull(gate.observe("com.tencent.mm", true, 2))
        assertNull(gate.observe(null, false, 0))
        assertNull(gate.observe("com.eg.android.AlipayGphone", true, 2))
        assertNotNull(gate.observe("com.eg.android.AlipayGphone", true, 2))
    }
}
