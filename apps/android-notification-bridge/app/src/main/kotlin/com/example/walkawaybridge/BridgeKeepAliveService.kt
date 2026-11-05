package com.example.walkawaybridge

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat

/**
 * Foreground keep-alive for the bus poller (WS3): while the bridge is
 * connected, this service holds foreground importance so Android 15's
 * cached-app freezer cannot freeze `com.example.walkawaybridge` when
 * another app comes to the front (observed 2026-11-04 19:04 PT).
 * Polling itself still runs in the Activity in this pass.
 */
class BridgeKeepAliveService : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val manager = getSystemService(NotificationManager::class.java)
    manager.createNotificationChannel(
      NotificationChannel(
        CHANNEL_ID,
        "Walkaway bridge connection",
        NotificationManager.IMPORTANCE_LOW
      ).apply { description = "Keeps the Walkaway laptop-to-glasses bridge connected" }
    )
    val notification = NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(android.R.drawable.ic_dialog_info)
      .setContentTitle("Walkaway Bridge connected")
      .setContentText("Polling the laptop bus for glasses cards")
      .setOngoing(true)
      .build()
    if (Build.VERSION.SDK_INT >= 34) {
      startForeground(NOTIF_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
    } else {
      startForeground(NOTIF_ID, notification)
    }
    return START_STICKY
  }

  companion object {
    private const val CHANNEL_ID = "walkaway_bridge_service"
    private const val NOTIF_ID = 4101

    fun start(context: Context) {
      ContextCompat.startForegroundService(
        context, Intent(context, BridgeKeepAliveService::class.java)
      )
    }

    fun stop(context: Context) {
      context.stopService(Intent(context, BridgeKeepAliveService::class.java))
    }
  }
}
