package com.example.walkawaybridge

import android.app.ForegroundServiceStartNotAllowedException
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat

/**
 * Foreground keep-alive AND owner of the bus poller (alignment 6.4.1).
 * While the bridge is connected, this service holds foreground importance
 * so Android 15's cached-app freezer cannot freeze
 * `com.example.walkawaybridge` when another app comes to the front
 * (observed 2026-11-04 19:04 PT), and the polling loop runs here instead
 * of in the Activity composition — so a bus `received` ack still arrives
 * after the Activity is destroyed (task removed).
 *
 * Type is `connectedDevice` (companion glasses link), NOT `dataSync`:
 * Android 15 caps dataSync foreground services (observed crash loop
 * 2026-11-05 22:29 PT: ForegroundServiceDidNotStopInTimeException, then a
 * START_STICKY restart into the exhausted quota). This service is
 * START_NOT_STICKY, stops itself on timeout/callback, and must never
 * crash the app if the system refuses foreground promotion — polling
 * then continues degraded on the process-wide [BridgePoller] scope.
 */
class BridgeKeepAliveService : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    BridgePoller.attach(this)
    val prefs = getSharedPreferences(BUS_PREFS, MODE_PRIVATE)
    val url = prefs.getString(PREF_SERVER, DEFAULT_BUS_SERVER) ?: DEFAULT_BUS_SERVER
    val token = prefs.getString(PREF_TOKEN, "") ?: ""
    // Start polling before foreground promotion so a refused FGS cannot
    // leave the bridge silently dead; the loop lives on BridgePoller's own
    // scope and outlives this service in that degraded case.
    BridgePoller.startPolling(url, token)

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
    try {
      if (Build.VERSION.SDK_INT >= 34) {
        startForeground(NOTIF_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
      } else {
        startForeground(NOTIF_ID, notification)
      }
    } catch (e: ForegroundServiceStartNotAllowedException) {
      Log.w(TAG, "Foreground promotion refused; polling continues degraded on BridgePoller", e)
      stopSelf()
    } catch (e: IllegalStateException) {
      Log.w(TAG, "Foreground promotion failed; polling continues degraded on BridgePoller", e)
      stopSelf()
    }
    return START_NOT_STICKY
  }

  override fun onTimeout(startId: Int, fgsType: Int) {
    Log.w(TAG, "Foreground service timeout (startId=$startId type=$fgsType); stopping cleanly")
    BridgePoller.stopPolling()
    stopForeground(STOP_FOREGROUND_REMOVE)
    stopSelf()
  }

  override fun onDestroy() {
    runCatching { stopForeground(STOP_FOREGROUND_REMOVE) }
    super.onDestroy()
  }

  companion object {
    private const val TAG = "walkaway.fgs"
    private const val CHANNEL_ID = "walkaway_bridge_service"
    private const val NOTIF_ID = 4101

    fun start(context: Context) {
      runCatching {
        ContextCompat.startForegroundService(
          context, Intent(context, BridgeKeepAliveService::class.java)
        )
      }.onFailure { Log.w(TAG, "Keep-alive start refused; continuing without it", it) }
    }

    fun stop(context: Context) {
      BridgePoller.stopPolling()
      runCatching { context.stopService(Intent(context, BridgeKeepAliveService::class.java)) }
    }
  }
}
