package com.example.localledger

import android.content.ContentValues
import android.provider.MediaStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

@RunWith(AndroidJUnit4::class)
class DocumentTransferInstrumentedTest {
    @Test fun contentResolverRoundTripAndOversizePreservesPreviousSelection() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val resolver=context.contentResolver
        val uri=resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI,ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME,"ledger-test-${System.nanoTime()}.meowm")
            put(MediaStore.MediaColumns.MIME_TYPE,"application/octet-stream")
        })!!
        val target=File(context.cacheDir,"document-transfer-test")
        try {
            val bytes=byteArrayOf(0,1,2,-1,10,13)
            resolver.openOutputStream(uri,"wt")!!.use { it.write(bytes) }
            DocumentTransfer.importUri(resolver,uri,target,16)
            assertArrayEquals(bytes,target.readBytes())
            try { DocumentTransfer.importUri(resolver,uri,target,2); fail("Oversized import accepted") }
            catch (_:java.io.IOException) { }
            assertArrayEquals(bytes,target.readBytes())
            assertFalse(File(target.parentFile,".${target.name}.part").exists())
        } finally { target.delete();resolver.delete(uri,null,null) }
    }
}
