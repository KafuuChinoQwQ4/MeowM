package com.example.localledger

import android.content.ContentResolver
import android.net.Uri
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream

/** SAF streams are bounded before Rust receives a backup or CSV payload. */
object DocumentTransfer {
    const val MAX_BACKUP_BYTES = 64L * 1024 * 1024
    const val MAX_CSV_BYTES = 32L * 1024 * 1024
    @Throws(IOException::class)
    fun copyBounded(input: InputStream, output: OutputStream, limit: Long) {
        val buffer=ByteArray(8192); var total=0L
        while (true) {
            val read=input.read(buffer)
            if (read < 0) break
            total += read
            if (total > limit) throw IOException("文件超过大小限制")
            output.write(buffer,0,read)
        }
        output.flush()
    }
    @Throws(IOException::class)
    fun importUri(resolver: ContentResolver, uri: Uri, destination: java.io.File, limit: Long) {
        val temporary=java.io.File(destination.parentFile, ".${destination.name}.part")
        try {
            resolver.openInputStream(uri)?.use { input -> temporary.outputStream().use { output -> copyBounded(input,output,limit) } }
                ?: throw IOException("无法打开文件")
            if (!temporary.renameTo(destination)) throw IOException("无法保存文件")
        } finally { temporary.delete() }
    }
}
