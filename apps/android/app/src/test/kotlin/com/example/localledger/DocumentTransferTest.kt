package com.example.localledger

import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream

class DocumentTransferTest {
    @Test fun boundedCopyRejectsOversizedDocuments() {
        val output=ByteArrayOutputStream()
        try { DocumentTransfer.copyBounded(ByteArrayInputStream(ByteArray(17)),output,16); fail("oversize document accepted") }
        catch (_: java.io.IOException) { }
    }
    @Test fun boundedCopyRetainsExactBytes() {
        val bytes=byteArrayOf(0,1,2,-1,13,10)
        val output=ByteArrayOutputStream()
        DocumentTransfer.copyBounded(ByteArrayInputStream(bytes),output,16)
        assertArrayEquals(bytes,output.toByteArray())
    }
}
