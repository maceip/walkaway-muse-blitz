package com.example.walkawaybridge

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat

const val WALKAWAY_CHANNEL_ID = "walkaway_alerts"
const val EXTRA_MESSAGE_ID = "walkaway.message_id"
const val EXTRA_MESSAGE_TITLE = "walkaway.message_title"
const val EXTRA_MESSAGE_BODY = "walkaway.message_body"

fun ensureWalkawayChannel(context: Context) {
  val manager = context.getSystemService(NotificationManager::class.java)
  val channel = NotificationChannel(
    WALKAWAY_CHANNEL_ID,
    "Walkaway approvals",
    NotificationManager.IMPORTANCE_HIGH
  ).apply {
    description = "Approval and status alerts from the Walkaway laptop bridge"
  }
  manager.createNotificationChannel(channel)
}

fun showWalkawayNotification(context: Context, message: BusMessage) {
  ensureWalkawayChannel(context)
  val openIntent = Intent(context, MainActivity::class.java).apply {
    flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
    putExtra(EXTRA_MESSAGE_ID, message.id)
    putExtra(EXTRA_MESSAGE_TITLE, message.title)
    putExtra(EXTRA_MESSAGE_BODY, message.body)
  }
  val pending = PendingIntent.getActivity(
    context,
    message.id.toInt(),
    openIntent,
    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
  )
  // No Approve/Reject actions: phone notifications are status-only.
  // Decisions count exclusively from physical glasses taps (glasses_dat);
  // the previous notification actions acked as notification_action, which
  // every decision path ignores — dead controls (alignment 6.1.1).
  val notification = NotificationCompat.Builder(context, WALKAWAY_CHANNEL_ID)
    .setSmallIcon(android.R.drawable.ic_dialog_info)
    .setContentTitle(message.title)
    .setContentText(message.body)
    .setStyle(NotificationCompat.BigTextStyle().bigText(message.body))
    .setPriority(NotificationCompat.PRIORITY_HIGH)
    .setCategory(NotificationCompat.CATEGORY_MESSAGE)
    .setContentIntent(pending)
    .setAutoCancel(true)
    .build()
  NotificationManagerCompat.from(context).notify(message.id.toInt(), notification)
}
