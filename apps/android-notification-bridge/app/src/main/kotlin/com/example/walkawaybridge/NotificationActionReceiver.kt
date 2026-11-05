package com.example.walkawaybridge

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationManagerCompat

const val BUS_PREFS = "walkaway_bus"
const val PREF_PENDING_ACK_ID = "pending_ack_id"
const val PREF_PENDING_ACK_ACTION = "pending_ack_action"

class NotificationActionReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    val messageId = intent.getLongExtra(EXTRA_MESSAGE_ID, -1L)
    val action = intent.getStringExtra("walkaway.action") ?: return
    if (messageId <= 0) return
    context.getSharedPreferences(BUS_PREFS, Context.MODE_PRIVATE)
      .edit()
      .putLong(PREF_PENDING_ACK_ID, messageId)
      .putString(PREF_PENDING_ACK_ACTION, action)
      .apply()
    NotificationManagerCompat.from(context).cancel(messageId.toInt())

    // Bring the polling activity forward so the queued acknowledgement sends
    // immediately while the transport proof runs in the foreground.
    val open = Intent(context, MainActivity::class.java).apply {
      flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
      putExtra(EXTRA_MESSAGE_ID, messageId)
    }
    context.startActivity(open)
  }
}
