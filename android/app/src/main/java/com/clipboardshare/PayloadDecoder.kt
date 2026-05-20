package com.clipboardshare

import java.io.DataInputStream
import java.io.InputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Decodes a single clipboard-share Payload frame from the given stream.
 *
 * Wire format (matches the Rust bincode serialization):
 *   [4 bytes big-endian]    frame length N
 *   [N bytes]               bincode-encoded Payload enum:
 *     [4 bytes little-endian] variant index  (0=Text, 1=Image, 2=Heartbeat)
 *     for Text:
 *       [8 bytes little-endian] string byte length L
 *       [L bytes]               UTF-8 string
 *
 * Returns the text string for a Text payload, null for Image / Heartbeat.
 * Throws IOException on read errors or malformed frames.
 */
object PayloadDecoder {

    private const val MAX_FRAME_BYTES = 32 * 1024 * 1024  // 32 MiB — matches Rust

    private const val VARIANT_TEXT = 0
    private const val VARIANT_IMAGE = 1
    private const val VARIANT_HEARTBEAT = 2

    fun decode(stream: InputStream): String? {
        val din = DataInputStream(stream)

        // Read 4-byte big-endian frame length. readInt() throws EOFException on disconnect.
        val frameLen = din.readInt()
        require(frameLen in 1..MAX_FRAME_BYTES) { "frame length out of range: $frameLen" }

        val frame = ByteArray(frameLen)
        din.readFully(frame)

        val buf = ByteBuffer.wrap(frame).order(ByteOrder.LITTLE_ENDIAN)

        return when (val variant = buf.int) {
            VARIANT_TEXT -> {
                val strLen = buf.long.toInt()
                require(strLen >= 0 && strLen <= frame.size - 12) { "invalid string length: $strLen" }
                String(frame, 12, strLen, Charsets.UTF_8)
            }
            VARIANT_IMAGE, VARIANT_HEARTBEAT -> null
            else -> null.also { android.util.Log.w("PayloadDecoder", "unknown variant $variant, skipping") }
        }
    }
}
