package com.example.localledger

import android.content.Context
import androidx.work.Worker
import androidx.work.WorkerParameters
import androidx.work.ListenableWorker
import org.json.JSONObject

class ExchangeRateWorker(context: Context, parameters: WorkerParameters) : Worker(context, parameters) {
    override fun doWork(): Result = try {
        resultForReply(RustBridge.nativeSyncExchangeRates(RustBridge.databasePath(applicationContext)))
    } catch (_: Exception) {
        Result.retry()
    }

    companion object {
        internal fun shouldRetry(reply: String): Boolean = try {
            val status = JSONObject(reply).optString("status")
            status != "success" && status != "no_new_data"
        } catch (_: Exception) {
            true
        }

        internal fun resultForReply(reply: String): ListenableWorker.Result =
            if (shouldRetry(reply)) ListenableWorker.Result.retry() else ListenableWorker.Result.success()
    }
}
