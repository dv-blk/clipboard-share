package com.clipboardshare

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import java.net.ServerSocket
import java.net.Socket
import kotlin.concurrent.thread

class ClipboardListenerService : Service() {

    private val tag = "ClipboardListenerService"
    private val channelId = "clipboard_share"

    @Volatile
    private var running = false
    private var serverSocket: ServerSocket? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }

        if (running) return START_STICKY

        val port = intent?.getIntExtra(EXTRA_PORT, DEFAULT_PORT) ?: DEFAULT_PORT

        createNotificationChannel()
        startForeground(NOTIFICATION_ID, buildNotification(port, connected = false))

        running = true
        thread(name = "clipboard-listener") { listenLoop(port) }

        return START_STICKY
    }

    override fun onDestroy() {
        running = false
        serverSocket?.runCatching { close() }
        super.onDestroy()
    }

    // --- listener loop ---

    private fun listenLoop(port: Int) {
        while (running) {
            try {
                ServerSocket(port).use { ss ->
                    serverSocket = ss
                    Log.i(tag, "listening on port $port")
                    while (running) {
                        val socket = ss.accept()
                        thread(name = "clipboard-conn") { handleConnection(socket, port) }
                    }
                }
            } catch (e: Exception) {
                if (running) {
                    Log.w(tag, "server socket error: ${e.message}, retrying in 2s")
                    Thread.sleep(2_000)
                }
            }
        }
    }

    private fun handleConnection(socket: Socket, port: Int) {
        Log.i(tag, "accepted connection from ${socket.remoteSocketAddress}")
        updateNotification(port, connected = true)
        try {
            socket.use {
                val stream = it.getInputStream()
                while (running && !it.isClosed) {
                    val text = PayloadDecoder.decode(stream) ?: continue
                    Log.d(tag, "received text (${text.length} chars)")
                    setClipboard(text)
                }
            }
        } catch (e: Exception) {
            Log.i(tag, "connection closed: ${e.message}")
        } finally {
            updateNotification(port, connected = false)
        }
    }

    // --- clipboard ---

    private fun setClipboard(text: String) {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("clipboard-share", text))
    }

    // --- notification ---

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            channelId,
            "Clipboard Share",
            NotificationManager.IMPORTANCE_LOW
        ).apply { description = "Clipboard sync service" }
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    private fun buildNotification(port: Int, connected: Boolean): Notification {
        val stopIntent = PendingIntent.getService(
            this, 0,
            Intent(this, ClipboardListenerService::class.java).apply { action = ACTION_STOP },
            PendingIntent.FLAG_IMMUTABLE
        )
        val openIntent = PendingIntent.getActivity(
            this, 0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE
        )
        val status = if (connected) "Connected" else "Waiting on :$port"
        return NotificationCompat.Builder(this, channelId)
            .setContentTitle("Clipboard Share")
            .setContentText(status)
            .setSmallIcon(android.R.drawable.ic_menu_share)
            .setContentIntent(openIntent)
            .addAction(android.R.drawable.ic_menu_close_clear_cancel, "Stop", stopIntent)
            .setOngoing(true)
            .build()
    }

    private fun updateNotification(port: Int, connected: Boolean) {
        val nm = getSystemService(NotificationManager::class.java)
        nm.notify(NOTIFICATION_ID, buildNotification(port, connected))
    }

    companion object {
        const val DEFAULT_PORT = 9876
        const val EXTRA_PORT = "port"
        const val ACTION_STOP = "com.clipboardshare.STOP"
        private const val NOTIFICATION_ID = 1
    }
}
