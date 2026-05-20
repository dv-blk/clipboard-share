package com.clipboardshare

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.provider.Settings
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import com.clipboardshare.databinding.ActivityMainBinding

class MainActivity : AppCompatActivity() {

    private lateinit var binding: ActivityMainBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        val prefs = getSharedPreferences("clipboard_share", Context.MODE_PRIVATE)

        // Restore saved port.
        val savedPort = prefs.getInt("port", ClipboardListenerService.DEFAULT_PORT)
        binding.portInput.setText(savedPort.toString())

        binding.toggleButton.setOnClickListener {
            val portText = binding.portInput.text.toString()
            val port = portText.toIntOrNull()?.takeIf { it in 1..65535 }
            if (port == null) {
                binding.portInput.error = "Enter a valid port (1–65535)"
                return@setOnClickListener
            }

            prefs.edit().putInt("port", port).apply()

            val serviceIntent = Intent(this, ClipboardListenerService::class.java).apply {
                putExtra(ClipboardListenerService.EXTRA_PORT, port)
            }
            ContextCompat.startForegroundService(this, serviceIntent)
        }

        binding.stopButton.setOnClickListener {
            val stopIntent = Intent(this, ClipboardListenerService::class.java).apply {
                action = ClipboardListenerService.ACTION_STOP
            }
            startService(stopIntent)
        }

        requestBatteryOptimizationExemption()
    }

    private fun requestBatteryOptimizationExemption() {
        val pm = getSystemService(PowerManager::class.java)
        if (!pm.isIgnoringBatteryOptimizations(packageName)) {
            val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS).apply {
                data = Uri.parse("package:$packageName")
            }
            startActivity(intent)
        }
    }
}
