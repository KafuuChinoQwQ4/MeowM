package com.example.localledger

import android.content.Context
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequest
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequest
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit

object ExchangeRateWorkScheduler {
    const val PERIODIC_WORK_NAME = "meowm-exchange-rates-daily"
    const val STARTUP_WORK_NAME = "meowm-exchange-rates-startup"

    private fun constraints() = Constraints.Builder()
        .setRequiredNetworkType(NetworkType.CONNECTED)
        .build()

    internal fun periodicRequest(): PeriodicWorkRequest = PeriodicWorkRequestBuilder<ExchangeRateWorker>(
        24, TimeUnit.HOURS,
    )
        .setConstraints(constraints())
        .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
        .addTag(PERIODIC_WORK_NAME)
        .build()

    private fun startupRequest(): OneTimeWorkRequest = OneTimeWorkRequestBuilder<ExchangeRateWorker>()
        .setConstraints(constraints())
        .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
        .addTag(STARTUP_WORK_NAME)
        .build()

    fun ensureScheduled(context: Context) {
        WorkManager.getInstance(context).enqueueUniquePeriodicWork(
            PERIODIC_WORK_NAME,
            ExistingPeriodicWorkPolicy.KEEP,
            periodicRequest(),
        )
    }

    fun enqueueStartupSync(context: Context) {
        WorkManager.getInstance(context).enqueueUniqueWork(
            STARTUP_WORK_NAME,
            ExistingWorkPolicy.KEEP,
            startupRequest(),
        )
    }
}
