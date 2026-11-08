package com.example.walkawaybridge

import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat

@Composable
fun BridgeScreen(modifier: Modifier = Modifier) {
  val context = LocalContext.current
  val activity = context as? ComponentActivity
  val prefs = remember { context.getSharedPreferences(BUS_PREFS, Context.MODE_PRIVATE) }
  val dat = remember { WalkawayDat.getInstance(context.applicationContext) }
  val datStatus by dat.status.collectAsState()
  // 6.4.1: polling lives in BridgeKeepAliveService/BridgePoller — this
  // screen only observes the shared state and issues user actions.
  val snap by BridgePoller.state.collectAsState()

  var serverUrl by remember { mutableStateOf(prefs.getString(PREF_SERVER, DEFAULT_BUS_SERVER) ?: DEFAULT_BUS_SERVER) }
  // No working default credential ships in the UI (alignment 6.1.5): a fresh
  // install starts blank and visibly fails auth until the real token is entered.
  var token by remember { mutableStateOf(prefs.getString(PREF_TOKEN, "") ?: "") }

  val phase = bridgePhase(snap.connected, snap.cardMessageId, snap.decidedMessageId)

  var datActionNote by remember { mutableStateOf("") }

  val btEnableLauncher = rememberLauncherForActivityResult(
    ActivityResultContracts.StartActivityForResult()
  ) {
    dat.refreshNow()
    val btNow = WalkawayDat.isBluetoothEnabled(context)
    datActionNote = if (btNow) "Bluetooth turned ON — reconnecting glasses…" else "Bluetooth is still OFF"
  }

  val datPermissions = rememberLauncherForActivityResult(
    ActivityResultContracts.RequestMultiplePermissions()
  ) { _ ->
    if (WalkawayDat.permissionsGranted(context) && activity != null) {
      if (!WalkawayDat.isBluetoothEnabled(context)) {
        WalkawayDat.ensureBluetoothEnabled(context, activity)
        if (!WalkawayDat.isBluetoothEnabled(context)) {
          runCatching { btEnableLauncher.launch(Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE)) }
        }
      }
      dat.initialize(activity)
      datActionNote = "DAT permissions granted"
    } else {
      datActionNote = "DAT permissions not granted"
    }
  }

  LaunchedEffect(Unit) {
    ensureWalkawayChannel(context)
    BridgePoller.attach(context)
    val missing = WalkawayDat.PERMISSIONS.filter {
      ContextCompat.checkSelfPermission(context, it) != PackageManager.PERMISSION_GRANTED
    }
    if (missing.isNotEmpty()) {
      datPermissions.launch(missing.toTypedArray())
    } else if (activity != null) {
      if (!WalkawayDat.isBluetoothEnabled(context)) {
        WalkawayDat.ensureBluetoothEnabled(context, activity)
        if (!WalkawayDat.isBluetoothEnabled(context)) {
          runCatching { btEnableLauncher.launch(Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE)) }
        }
      }
      dat.initialize(activity)
    }
    // Demo-safe default (WS3): launching the Activity auto-connects, which
    // now means starting the poller-owning keep-alive service.
    BridgeKeepAliveService.start(context)
  }

  LaunchedEffect(datStatus.initialized) {
    if (datStatus.initialized) dat.refreshNow()
  }

  val activeCardSummary = when {
    snap.cardMessageId != null -> "Card #${snap.cardMessageId} LIVE ON GLASSES (${snap.cardMessage?.title ?: ""})"
    snap.pendingResend != null -> "Card #${snap.pendingResend?.id} QUEUED / NEEDS RESEND (${snap.deliveryState})"
    snap.pendingDecision != null -> "Card #${snap.pendingDecision?.id} IN FLIGHT (${snap.deliveryState})"
    snap.decidedMessageId != null -> "Card #${snap.decidedMessageId} DECIDED: ${snap.decidedAction} via ${snap.decidedSurface}"
    else -> "None (Bus cursor at #${snap.lastId})"
  }

  Column(
    modifier = modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp),
    verticalArrangement = Arrangement.spacedBy(14.dp)
  ) {
    Text("Walkaway Bridge", style = MaterialTheme.typography.headlineMedium)
    Text("Laptop → Android → glasses notification proof")
    BridgeStatusPanel(
      phase = phase,
      detail = when (phase) {
        BridgePhase.CARD_ON_GLASSES -> "Card #${snap.cardMessageId} is live. Decide on glasses or phone — first tap wins."
        BridgePhase.DECIDED -> "Card #${snap.decidedMessageId} decided via ${snap.decidedSurface ?: "unknown"}: ${snap.decidedAction}"
        else -> snap.status
      },
      datLine = "Bluetooth: ${if (datStatus.bluetoothEnabled) "ON" else "OFF (Glasses link blocked)"}\n" +
        "Glasses: ${datStatus.connectedDevice ?: "DISCONNECTED"} (reg=${datStatus.registration}, known=${datStatus.deviceCount})\n" +
        "Queue: $activeCardSummary" +
        (if (snap.skippedStaleCount > 0) " [Skipped ${snap.skippedStaleCount} stale]" else "") +
        "\nEvent: ${datStatus.lastEvent}"
    )
    if (!datStatus.bluetoothEnabled) {
      Button(
        onClick = {
          WalkawayDat.ensureBluetoothEnabled(context, activity)
          if (!WalkawayDat.isBluetoothEnabled(context)) {
            runCatching { btEnableLauncher.launch(Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE)) }
          }
        },
        modifier = Modifier.fillMaxWidth()
      ) {
        Text("Turn On Bluetooth for Glasses")
      }
    }
    OutlinedButton(
      onClick = { BridgePoller.resyncAndRetryLatest() },
      modifier = Modifier.fillMaxWidth()
    ) {
      Text("Resync Queue & Retry Latest Card")
    }
    OutlinedTextField(
      value = serverUrl,
      onValueChange = { serverUrl = it },
      label = { Text("Laptop bus URL") },
      modifier = Modifier.fillMaxWidth(),
      singleLine = true
    )
    OutlinedTextField(
      value = token,
      onValueChange = { token = it },
      label = { Text("Bus token") },
      modifier = Modifier.fillMaxWidth(),
      singleLine = true
    )
    Button(
      onClick = {
        prefs.edit().putString(PREF_SERVER, serverUrl.trim()).putString(PREF_TOKEN, token.trim()).apply()
        if (snap.connected) BridgeKeepAliveService.stop(context) else BridgeKeepAliveService.start(context)
      },
      modifier = Modifier.fillMaxWidth()
    ) {
      Text(if (snap.connected) "Disconnect" else "Connect")
    }
    Button(onClick = {
      val host = activity
      if (host == null) {
        datActionNote = "No activity available for DAT registration"
      } else if (!WalkawayDat.permissionsGranted(context)) {
        datPermissions.launch(WalkawayDat.PERMISSIONS)
      } else {
        dat.initialize(host)
        dat.refreshNow()
        dat.startRegistration(host)
      }
    }) { Text("Connect Meta AI") }
    if (datActionNote.isNotEmpty()) {
      Text(datActionNote, style = MaterialTheme.typography.bodyMedium)
    }
    snap.pendingResend?.let { message ->
      Button(
        onClick = { BridgePoller.resend(message) },
        modifier = Modifier.fillMaxWidth()
      ) { Text("Resend card #${message.id}") }
    }
    snap.lastMessage?.let { message ->
      Text("Latest card #${message.id} ${message.title}", style = MaterialTheme.typography.titleLarge)
      Text(message.body)
      // Page-preview crop captured by the browser pipeline (max 320px wide).
      val preview = remember(message.id) {
        message.imageB64?.let { b64 ->
          runCatching {
            val bytes = android.util.Base64.decode(b64, android.util.Base64.NO_WRAP)
            android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
          }.getOrNull()
        }
      }
      preview?.let { bmp ->
        androidx.compose.foundation.Image(
          bitmap = bmp.asImageBitmap(),
          contentDescription = "Page preview",
          modifier = Modifier.fillMaxWidth().height(160.dp)
        )
      }
      if (snap.pendingDecision?.id == message.id) {
        // Dual-surface approval (alignment §9): phone decisions ack as
        // android_app; the bridge records surface="android_app" and the
        // first tap across glasses/phone wins. DAT card is dismissed so
        // both surfaces clear together.
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
          Button(onClick = { BridgePoller.decideOnPhone(message, "approve") }) { Text("Approve on Phone") }
          OutlinedButton(onClick = { BridgePoller.decideOnPhone(message, "reject") }) { Text("Reject on Phone") }
        }
      } else if (snap.decidedMessageId == message.id) {
        // Decided cards must not show decision prompts again (first tap wins).
        Text("Decided: ${snap.decidedAction} via ${snap.decidedSurface ?: "unknown"}")
      } else {
        Text("Decide on glasses or phone — first tap wins.")
      }
    }
  }
}
