package com.example.localledger

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.work.NetworkType
import androidx.work.WorkManager
import androidx.work.ListenableWorker
import androidx.work.testing.WorkManagerTestInitHelper
import androidx.work.Configuration
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class ExchangeRateWorkSchedulerTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun dailyWorkRequiresNetworkAndKeepsOneRegistrationAndRetriesFailures() {
        WorkManagerTestInitHelper.initializeTestWorkManager(context, Configuration.Builder().build())
        assertFalse(ExchangeRateWorker.shouldRetry("{\"status\":\"success\"}"))
        assertFalse(ExchangeRateWorker.shouldRetry("{\"status\":\"no_new_data\"}"))
        assertTrue(ExchangeRateWorker.shouldRetry("{\"status\":\"timeout\"}"))
        assertTrue(ExchangeRateWorker.shouldRetry("invalid"))
        val request = ExchangeRateWorkScheduler.periodicRequest()
        assertEquals(TimeUnit.HOURS.toMillis(24), request.workSpec.intervalDuration)
        assertEquals(NetworkType.CONNECTED, request.workSpec.constraints.requiredNetworkType)
        ExchangeRateWorkScheduler.ensureScheduled(context)
        ExchangeRateWorkScheduler.ensureScheduled(context)
        val infos = WorkManager.getInstance(context)
            .getWorkInfosForUniqueWork(ExchangeRateWorkScheduler.PERIODIC_WORK_NAME).get()
        assertEquals(1, infos.size)
        assertTrue(infos.single().tags.contains(ExchangeRateWorkScheduler.PERIODIC_WORK_NAME))
    }
}
