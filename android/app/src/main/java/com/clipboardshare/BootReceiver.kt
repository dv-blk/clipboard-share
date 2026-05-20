package com.clipboardshare

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat

class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return

        val prefs = context.getSharedPreferences("clipboard_share", Context.MODE_PRIVATE)
        val port = prefs.getInt("port", ClipboardListenerService.DEFAULT_PORT)

        val serviceIntent = Intent(context, ClipboardListenerService::class.java).apply {
            putExtra(ClipboardListenerService.EXTRA_PORT, port)
        }
        ContextCompat.startForegroundService(context, serviceIntent)
    }
}
