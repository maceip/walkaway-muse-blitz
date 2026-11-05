package com.example.walkawaybridge

import android.Manifest
import android.util.Log
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
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
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

const val BUS_PREFS = "walkaway_bus"
private const val PREF_SERVER = "server_url"
private const val PREF_TOKEN = "bus_token"
private const val PREF_LAST_ID = "last_message_id"
private const val PREF_CARDED_IDS = "carded_message_ids"
private const val PREF_DECIDED_IDS = "decided_message_ids"

@Composable
fun BridgeScreen(modifier: Modifier = Modifier) {
  val context = LocalContext.current
  val activity = context as? ComponentActivity
  val prefs = remember { context.getSharedPreferences(BUS_PREFS, Context.MODE_PRIVATE) }
  val scope = rememberCoroutineScope()
  val dat = remember { WalkawayDat.getInstance(context.applicationContext) }
  val datStatus by dat.status.collectAsState()

  var serverUrl by remember { mutableStateOf(prefs.getString(PREF_SERVER, "http://127.0.0.1:19099") ?: "http://127.0.0.1:19099") }
  // No working default credential ships in the UI (alignment 6.1.5): a fresh
  // install starts blank and visibly fails auth until the real token is entered.
  var token by remember { mutableStateOf(prefs.getString(PREF_TOKEN, "") ?: "") }
  var connected by remember { mutableStateOf(true) }
  var status by remember { mutableStateOf("Auto-connecting…") }
  var lastMessage by remember { mutableStateOf<BusMessage?>(null) }
  var lastId by remember { mutableLongStateOf(prefs.getLong(PREF_LAST_ID, 0L)) }
  var cardMessageId by remember { mutableStateOf<Long?>(null) }
  var cardMessage by remember { mutableStateOf<BusMessage?>(null) }
  var pendingDecision by remember { mutableStateOf<BusMessage?>(null) }
  var pendingResend by remember { mutableStateOf<BusMessage?>(null) }
  var decidedMessageId by remember { mutableStateOf<Long?>(null) }
  var decidedAction by remember { mutableStateOf<String?>(null) }
  var decidedSurface by remember { mutableStateOf<String?>(null) }

  fun prefsIdDedupe(key: String) = IdDedupe(
    load = { prefs.getStringSet(key, emptySet()).orEmpty().mapNotNull { it.toLongOrNull() }.toSet() },
    save = { ids -> prefs.edit().putStringSet(key, ids.map { it.toString() }.toSet()).apply() }
  )
  val cardedDedupe = remember { prefsIdDedupe(PREF_CARDED_IDS) }
  val decidedDedupe = remember { prefsIdDedupe(PREF_DECIDED_IDS) }
  val phase = bridgePhase(connected, cardMessageId, decidedMessageId)

  val datPermissions = rememberLauncherForActivityResult(
    ActivityResultContracts.RequestMultiplePermissions()
  ) { _ ->
    if (WalkawayDat.permissionsGranted(context) && activity != null) {
      dat.initialize(activity)
      status = "DAT permissions granted"
    } else {
      status = "DAT permissions not granted"
    }
  }

  LaunchedEffect(Unit) {
    ensureWalkawayChannel(context)
    val missing = WalkawayDat.PERMISSIONS.filter {
      ContextCompat.checkSelfPermission(context, it) != PackageManager.PERMISSION_GRANTED
    }
    if (missing.isNotEmpty()) {
      datPermissions.launch(missing.toTypedArray())
    } else if (activity != null) {
      dat.initialize(activity)
    }
  }

  LaunchedEffect(datStatus.initialized) {
    if (datStatus.initialized) dat.refreshNow()
  }

  fun client() = BusClient(serverUrl.trim(), token.trim())

  fun sendAckFor(message: BusMessage, action: String, source: String) {
    scope.launch {
      runCatching { withContext(Dispatchers.IO) { client().ack(message.id, action, source) } }
        .onSuccess { status = "Ack sent: $action for #${message.id} via $source" }
        .onFailure { status = "Ack failed: ${it.message}" }
    }
  }

  fun sendDat(message: BusMessage) {
    if (!cardedDedupe.mark(message.id)) {
      status = "Duplicate card suppressed for #${message.id}"
      return
    }
    cardMessage = message
    dat.sendCard(
      title = message.title,
      body = message.body,
      onAction = { action ->
        if (decidedDedupe.mark(message.id)) {
          cardMessageId = null
          cardMessage = null
          pendingDecision = null
          pendingResend = null
          decidedMessageId = message.id
          decidedAction = action
          decidedSurface = "glasses"
          sendAckFor(message, action, "glasses_dat")
        }
      },
      onResult = { result ->
        result.onSuccess {
          cardMessageId = message.id
          decidedMessageId = null
          decidedAction = null
          decidedSurface = null
          status = "DAT card sent for #${message.id}"
          sendAckFor(message, "dat_sent", "android_dat")
        }.onFailure {
          // Failed sends must not consume the one-card dedupe slot (WS3).
          cardedDedupe.forget(message.id)
          pendingResend = message
          status = "DAT card failed: ${it.message}${datFailureHint(it.message)} Use Resend card."
          sendAckFor(message, "dat_failed", "android_dat")
        }
      }
    )
  }

  fun decideOnPhone(message: BusMessage, action: String) {
    if (!decidedDedupe.mark(message.id)) return
    cardMessageId = null
    cardMessage = null
    pendingDecision = null
    pendingResend = null
    decidedMessageId = message.id
    decidedAction = action
    decidedSurface = "android_app"
    dat.dismissActiveCard()
    sendAckFor(message, action, "android_app")
  }

  LaunchedEffect(dat) {
    dat.onCardGone = {
      val gone = cardMessage
      if (gone != null && decidedMessageId != gone.id) {
        cardedDedupe.forget(gone.id)
        cardMessageId = null
        pendingResend = gone
        status = "Card #${gone.id} gone without a decision. Use Resend card."
      }
    }
  }

  LaunchedEffect(connected) {
    if (connected) BridgeKeepAliveService.start(context) else BridgeKeepAliveService.stop(context)
  }

  LaunchedEffect(connected, serverUrl, token) {
    if (!connected) return@LaunchedEffect
    Log.i("walkaway.poll", "effect start url=$serverUrl lastId=$lastId")
    status = "Connecting…"
    runCatching { withContext(Dispatchers.IO) { client().health() } }
      .onSuccess { status = "Connected. Waiting for laptop messages…" }
      .onFailure { status = "Connect failed: ${it.message}" }
    while (connected) {
      runCatching {
        withContext(Dispatchers.IO) {
          val bus = client()
          bus.nextMessage(lastId)
        }
      }.onSuccess { message ->
        if (message != null) {
          lastId = message.id
          prefs.edit().putLong(PREF_LAST_ID, message.id).apply()
          lastMessage = message
          pendingDecision = message
          showWalkawayNotification(context, message)
          status = "Received #${message.id}; notification posted; sending DAT card"
          sendDat(message)
          scope.launch {
            runCatching { withContext(Dispatchers.IO) { client().ack(message.id, "received", "android_app") } }
          }
        }
      }.onFailure {
        Log.e("walkaway.poll", "poll failed", it)
        status = "Poll failed: ${it.message}"
      }
      delay(1000)
    }
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
        BridgePhase.CARD_ON_GLASSES -> "Card #${cardMessageId} is live. Decide on glasses or phone — first tap wins."
        BridgePhase.DECIDED -> "Card #${decidedMessageId} decided via ${decidedSurface ?: "unknown"}: ${decidedAction}"
        else -> status
      },
      datLine = "DAT: registration=${datStatus.registration} devices=${datStatus.deviceCount} " +
        "connected=${datStatus.connectedDevice ?: "none"}\n${datStatus.lastEvent}"
    )
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
        connected = !connected
        if (!connected) status = "Not connected"
      },
      modifier = Modifier.fillMaxWidth()
    ) {
      Text(if (connected) "Disconnect" else "Connect")
    }
    Button(onClick = {
      val host = activity
      if (host == null) {
        status = "No activity available for DAT registration"
      } else if (!WalkawayDat.permissionsGranted(context)) {
        datPermissions.launch(WalkawayDat.PERMISSIONS)
      } else {
        dat.initialize(host)
        dat.refreshNow()
        dat.startRegistration(host)
      }
    }) { Text("Connect Meta AI") }
    pendingResend?.let { message ->
      Button(
        onClick = {
          pendingResend = null
          sendDat(message)
        },
        modifier = Modifier.fillMaxWidth()
      ) { Text("Resend card #${message.id}") }
    }
    lastMessage?.let { message ->
      Text("Latest card #${message.id} ${message.title}", style = MaterialTheme.typography.titleLarge)
      Text(message.body)
      if (pendingDecision?.id == message.id) {
        // Dual-surface approval (alignment §9): phone decisions ack as
        // android_app; the bridge records surface="android_app" and the
        // first tap across glasses/phone wins. DAT card is dismissed so
        // both surfaces clear together.
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
          Button(onClick = { decideOnPhone(message, "approve") }) { Text("Approve on Phone") }
          OutlinedButton(onClick = { decideOnPhone(message, "reject") }) { Text("Reject on Phone") }
        }
      } else {
        Text("Decide on glasses or phone — first tap wins.")
      }
    }
  }
}
