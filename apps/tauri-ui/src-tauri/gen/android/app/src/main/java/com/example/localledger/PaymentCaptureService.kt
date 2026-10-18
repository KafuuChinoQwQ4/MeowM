package com.example.localledger

import android.app.*
import android.content.*
import android.content.pm.ServiceInfo
import android.graphics.Bitmap
import android.graphics.PixelFormat
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.Image
import android.media.ImageReader
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.*
import android.view.WindowManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.chinese.ChineseTextRecognizerOptions
import org.json.JSONObject
import java.util.concurrent.Executor
import java.util.concurrent.atomic.AtomicBoolean

/** All frames, OCR completions and projection resources are serialized on worker. */
class PaymentCaptureService : Service() {
    private val stopped = AtomicBoolean(false)
    private lateinit var thread: HandlerThread
    private lateinit var worker: Handler
    private lateinit var foreground: PaymentForeground
    private val recognizer = TextRecognition.getClient(ChineseTextRecognizerOptions.Builder().build())
    private val gate = PaymentScreenGate()
    private var projection: MediaProjection? = null
    private var display: VirtualDisplay? = null
    private var reader: ImageReader? = null
    private var processing = false
    private var workerClosed = false
    private var pendingImage: Image? = null
    private var pendingPackage: String? = null
    private var pendingCaptured = 0L
    private var latestFrame: Bitmap? = null
    private var latestPackage: String? = null
    private var latestCaptured = 0L
    private var latestAt = 0L
    private var latestSamples = 0
    private var failures = 0
    private var width = 0
    private var height = 0
    @Volatile private var session = ""
    private val callback = object : MediaProjection.Callback() {
        override fun onStop() { finish("permission_revoked") }
        override fun onCapturedContentResize(w: Int, h: Int) {
            try { if (!stopped.get()) resize(w, h) } catch (_: Exception) { finish("error") }
        }
    }
    private val check = object : Runnable {
        override fun run() {
            if (stopped.get()) return
            try {
                val power = getSystemService(PowerManager::class.java)
                val keyguard = getSystemService(KeyguardManager::class.java)
                if (!power.isInteractive || keyguard.isKeyguardLocked) { finish("screen_locked"); return }
                if (!notificationsAllowed(this@PaymentCaptureService)) { finish("notification_required"); return }
                if (!PaymentForeground.granted(this@PaymentCaptureService)) { finish("usage_revoked"); return }
                if (!RustBridge.nativeScreenAllowed(session, "")) { finish("stopped"); return }
                if (Build.VERSION.SDK_INT < 34) {
                    @Suppress("DEPRECATION") val metrics = android.util.DisplayMetrics().also {
                        (getSystemService(WINDOW_SERVICE) as WindowManager).defaultDisplay.getRealMetrics(it)
                    }
                    resize(metrics.widthPixels, metrics.heightPixels)
                }
                sampleLatest()
                worker.postDelayed(this, 1000)
            } catch (_: Exception) { finish("error") }
              catch (_: LinkageError) { finish("error") }
        }
    }
    override fun onCreate() {
        super.onCreate()
        thread = HandlerThread("payment-screen-ocr").also { it.start() }
        worker = Handler(thread.looper)
        foreground = PaymentForeground(this)
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "支付页面识别", NotificationManager.IMPORTANCE_LOW))
    }
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) { finish("stopped"); return START_NOT_STICKY }
        if (session.isNotEmpty() || stopped.get()) return START_NOT_STICKY
        session = intent?.getStringExtra("session").orEmpty()
        @Suppress("DEPRECATION") val data = intent?.getParcelableExtra<Intent>("result_data")
        if (data == null || session.isEmpty() || intent.getIntExtra("result_code", 0) != Activity.RESULT_OK) {
            finish("permission_revoked"); return START_NOT_STICKY
        }
        try {
            if (Build.VERSION.SDK_INT >= 29) startForeground(NOTIFICATION_ID, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION)
            else startForeground(NOTIFICATION_ID, notification())
            worker.post {
                try {
                    if (stopped.get() || !RustBridge.nativeScreenAllowed(session, "")) { finish("stopped"); return@post }
                    val manager = getSystemService(MEDIA_PROJECTION_SERVICE) as MediaProjectionManager
                    projection = manager.getMediaProjection(Activity.RESULT_OK, data)
                    projection!!.registerCallback(callback, worker)
                    val metrics = resources.displayMetrics
                    resize(metrics.widthPixels, metrics.heightPixels)
                    // A token creates exactly one display. Rotation resizes this display.
                    display = projection!!.createVirtualDisplay("PaymentScreenCapture", width, height, metrics.densityDpi,
                        DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, reader!!.surface, null, worker)
                    if (stopped.get()) return@post
                    running = true; code = "running"
                    worker.post(check)
                } catch (_: Exception) { finish("error") }
                  catch (_: LinkageError) { finish("error") }
            }
        } catch (_: Exception) { finish("permission_revoked") }
        return START_NOT_STICKY
    }
    private fun resize(w: Int, h: Int) {
        if (w <= 0 || h <= 0) return
        val scale = minOf(1f, 1280f / maxOf(w, h))
        val nextW = (w * scale).toInt().coerceAtLeast(1)
        val nextH = (h * scale).toInt().coerceAtLeast(1)
        if (nextW == width && nextH == height) return
        val next = ImageReader.newInstance(nextW, nextH, PixelFormat.RGBA_8888, 3)
        next.setOnImageAvailableListener({ processLatest(it) }, worker)
        display?.resize(nextW, nextH, resources.displayMetrics.densityDpi)
        display?.surface = next.surface
        clearFrames()
        reader?.setOnImageAvailableListener(null, null)
        reader?.close()
        reader = next; width = nextW; height = nextH
    }
    private fun clearFrames() {
        pendingImage?.close(); pendingImage = null; pendingPackage = null
        latestFrame?.recycle(); latestFrame = null; latestPackage = null
    }
    private fun processLatest(source: ImageReader) {
        var image: Image? = null
        try {
            image = source.acquireLatestImage() ?: return
            val current = foreground.currentPackage()
            if (stopped.get() || current == null || !RustBridge.nativeScreenAllowed(session, current)) {
                clearFrames(); gate.observe(null, false, 0); return
            }
            // Keep only the latest image until the timer consumes it. Static displays need not
            // emit another frame, so dropping a throttled frame could miss an entire payment.
            pendingImage?.close()
            pendingImage = image; image = null
            pendingPackage = current; pendingCaptured = System.currentTimeMillis()
        } catch (_: Exception) { failed() }
          catch (_: LinkageError) { finish("error") }
          finally { image?.close() }
    }
    private fun sampleLatest() {
        if (stopped.get()) return
        val current = foreground.currentPackage()
        if (current == null || !RustBridge.nativeScreenAllowed(session, current)) {
            clearFrames(); gate.observe(null, false, 0); return
        }
        if (latestPackage != null && latestPackage != current) {
            latestFrame?.recycle(); latestFrame = null; latestPackage = null
            gate.observe(null, false, 0)
        }
        if (processing) return
        var frame: Bitmap? = null
        try {
            val next = pendingImage
            pendingImage = null
            if (next != null) next.use {
                if (pendingPackage == current) {
                    latestFrame?.recycle(); latestFrame = null
                    latestFrame = copyFrame(next)
                    latestPackage = current; latestCaptured = pendingCaptured
                    latestAt = SystemClock.elapsedRealtime(); latestSamples = 0
                }
            }
            // A cold OCR can take several seconds: allow its second observation before expiry.
            // Bound retention even if OCR never completes or repeatedly fails.
            val age = SystemClock.elapsedRealtime() - latestAt
            if (age > 30_000 || (latestSamples >= 2 && age > 3000)) {
                latestFrame?.recycle(); latestFrame = null; return
            }
            frame = latestFrame?.copy(Bitmap.Config.ARGB_8888, false) ?: return
            val bitmap = frame
            val captured = latestCaptured
            val packageName = current
            processing = true
            latestSamples++
            recognizer.process(InputImage.fromBitmap(bitmap, 0))
                .addOnCompleteListener(Executor { worker.post(it) }) { task ->
                    try {
                        if (!stopped.get()) {
                            if (!task.isSuccessful) { failed() }
                            else if (foreground.currentPackage() == packageName && RustBridge.nativeScreenAllowed(session, packageName)) {
                                val text = task.result.text.take(2048)
                                val id = gate.observe(packageName, SUCCESS.any(text::contains), text.hashCode())
                                if (id != null) {
                                    val event = JSONObject().put("event_id", "$session:$id").put("package_name", packageName)
                                        .put("recognized_text", text).put("captured_at_ms", captured).toString()
                                    val reply = JSONObject(RustBridge.nativeScreenCapture(session, event)).optString("code", "error")
                                    if (reply == "stopped") finish("stopped")
                                    else if (reply == "error") { gate.retry(); failed() }
                                    else { failures = 0; code = reply }
                                }
                            } else gate.observe(null, false, 0)
                        }
                    } catch (_: Exception) { gate.retry(); failed() }
                      catch (_: LinkageError) { finish("error") }
                    finally {
                        bitmap.recycle(); processing = false
                        if (stopped.get()) closeWorker()
                    }
                }
        } catch (_: Exception) {
            frame?.recycle(); processing = false; failed()
        } catch (_: LinkageError) {
            frame?.recycle(); processing = false; finish("error")
        }
    }
    private fun copyFrame(image: Image): Bitmap {
        val plane = image.planes[0]
        require(plane.pixelStride == 4 && plane.rowStride >= image.width * 4)
        val pixels = IntArray(image.width * image.height)
        val buffer = plane.buffer
        for (y in 0 until image.height) for (x in 0 until image.width) {
            val offset = y * plane.rowStride + x * plane.pixelStride
            val r = buffer.get(offset).toInt() and 255
            val g = buffer.get(offset + 1).toInt() and 255
            val b = buffer.get(offset + 2).toInt() and 255
            val a = buffer.get(offset + 3).toInt() and 255
            pixels[y * image.width + x] = (a shl 24) or (r shl 16) or (g shl 8) or b
        }
        return Bitmap.createBitmap(pixels, image.width, image.height, Bitmap.Config.ARGB_8888).also { pixels.fill(0) }
    }
    private fun failed() { failures++; if (failures >= 3) finish("error") }
    private fun finish(reason: String) {
        if (!stopped.compareAndSet(false, true)) return
        running = false; code = reason
        try { RustBridge.nativeScreenStop(session) } catch (_: LinkageError) { } catch (_: Exception) { }
        worker.post {
            running = false
            worker.removeCallbacks(check)
            clearFrames()
            val current = projection; projection = null
            current?.unregisterCallback(callback)
            reader?.setOnImageAvailableListener(null, null)
            display?.release(); display = null
            reader?.close(); reader = null
            current?.stop()
            if (!processing) closeWorker()
        }
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }
    private fun closeWorker() {
        if (workerClosed) return
        workerClosed = true
        recognizer.close(); thread.quitSafely()
    }
    override fun onDestroy() { finish("stopped"); super.onDestroy() }
    override fun onBind(intent: Intent?): IBinder? = null
    private fun notification(): Notification {
        val stop = PendingIntent.getService(this, 0, Intent(this, PaymentCaptureService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        return NotificationCompat.Builder(this, CHANNEL_ID).setSmallIcon(android.R.drawable.ic_menu_view)
            .setContentTitle("支付页面识别已开启").setContentText("在本机识别支付页面，点击停止可结束捕获")
            .setOngoing(true).addAction(android.R.drawable.ic_media_pause, "停止识别", stop).build()
    }
    companion object {
        fun notificationsAllowed(context: Context): Boolean {
            val channel = context.getSystemService(NotificationManager::class.java).getNotificationChannel(CHANNEL_ID)
            return NotificationManagerCompat.from(context).areNotificationsEnabled() && channel?.importance != NotificationManager.IMPORTANCE_NONE
        }
        const val ACTION_STOP = "com.example.localledger.PAYMENT_CAPTURE_STOP"
        const val CHANNEL_ID = "payment-screen-capture"
        const val NOTIFICATION_ID = 4102
        val SUCCESS = listOf("支付成功", "付款成功", "收款成功", "转账成功", "退款成功")
        @Volatile var running = false
            private set
        @Volatile var code = "stopped"
            private set
    }
}
