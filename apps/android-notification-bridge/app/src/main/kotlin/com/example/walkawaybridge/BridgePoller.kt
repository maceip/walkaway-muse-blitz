package com.example.walkawaybridge

import android.content.Context
import android.content.SharedPreferences
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

const val BUS_PREFS = "walkaway_bus"
const val PREF_SERVER = "server_url"
const val PREF_TOKEN = "bus_token"
const val PREF_LAST_ID = "last_message_id"
const val PREF_CARDED_IDS = "carded_message_ids"
const val PREF_DECIDED_IDS = "decided_message_ids"

const val DEFAULT_BUS_SERVER = "http://127.0.0.1:19099"

/**
 * Process-wide bus poller and shared bridge state (alignment 6.4.1).
 *
 * The polling loop no longer lives in the Activity composition:
 * [BridgeKeepAliveService] starts and stops it, so a bus `received` ack
 * still arrives after the Activity is destroyed (task removed). UI state
 * is a [StateFlow] — the Compose screen is a pure observer and control
 * surface. The loop runs on this object's own scope so it can survive the
 * service stopping itself when foreground promotion is refused (degraded
 * mode, no importance bump) instead of dying with the Activity.
 */
object BridgePoller {
  data class Snapshot(
    val connected: Boolean = false,
    val status: String = "Not connected",
    val lastMessage: BusMessage? = null,
    val cardMessageId: Long? = null,
    val cardMessage: BusMessage? = null,
    val pendingDecision: BusMessage? = null,
    val pendingResend: BusMessage? = null,
    val decidedMessageId: Long? = null,
    val decidedAction: String? = null,
    val decidedSurface: String? = null,
    val lastId: Long = 0L,
  )

  private val _state = MutableStateFlow(Snapshot())
  val state: StateFlow<Snapshot> = _state.asStateFlow()

  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
  @Volatile private var pollJob: Job? = null
  @Volatile private var serverUrl: String = ""
  @Volatile private var token: String = ""
  private var appContext: Context? = null
  private var dat: WalkawayDat? = null
  private var cardedDedupe: IdDedupe? = null
  private var decidedDedupe: IdDedupe? = null

  @Synchronized
  fun attach(context: Context) {
    if (appContext != null) return
    val app = context.applicationContext
    appContext = app
    val prefs = prefs(app)
    cardedDedupe = prefsIdDedupe(prefs, PREF_CARDED_IDS)
    decidedDedupe = prefsIdDedupe(prefs, PREF_DECIDED_IDS)
    dat = WalkawayDat.getInstance(app).also { d ->
      d.onCardGone = {
        val gone = _state.value.cardMessage
        if (gone != null && _state.value.decidedMessageId != gone.id) {
          cardedDedupe?.forget(gone.id)
          _state.update {
            it.copy(
              cardMessageId = null,
              pendingResend = gone,
              status = "Card #${gone.id} gone without a decision. Use Resend card.",
            )
          }
        }
      }
    }
    _state.update { it.copy(lastId = prefs.getLong(PREF_LAST_ID, 0L)) }
  }

  /** Idempotent: restarts the loop only when the target or credentials change. */
  @Synchronized
  fun startPolling(url: String, tok: String) {
    val running = pollJob
    if (running != null && running.isActive && serverUrl == url && token == tok) {
      _state.update { it.copy(connected = true) }
      return
    }
    serverUrl = url
    token = tok
    _state.update { it.copy(connected = true, status = "Connecting…") }
    pollJob?.cancel()
    pollJob = scope.launch {
      Log.i(TAG, "poller start url=$url lastId=${_state.value.lastId}")
      runCatching { withContext(Dispatchers.IO) { client().health() } }
        .onSuccess {
          _state.update { s -> s.copy(status = "Connected. Waiting for laptop messages…") }
        }
        .onFailure {
          _state.update { s -> s.copy(status = "Connect failed: ${it.message}") }
        }
      while (isActive && _state.value.connected) {
        runCatching {
          withContext(Dispatchers.IO) { client().nextMessage(_state.value.lastId) }
        }.onSuccess { message ->
          if (message != null) handleMessage(message)
        }.onFailure {
          Log.e(TAG, "poll failed", it)
          _state.update { s -> s.copy(status = "Poll failed: ${it.message}") }
        }
        delay(1000)
      }
    }
  }

  @Synchronized
  fun stopPolling() {
    _state.update { it.copy(connected = false, status = "Not connected") }
    pollJob?.cancel()
    pollJob = null
  }

  fun decideOnPhone(message: BusMessage, action: String) {
    if (decidedDedupe?.mark(message.id) != true) return
    _state.update {
      it.copy(
        cardMessageId = null,
        cardMessage = null,
        pendingDecision = null,
        pendingResend = null,
        decidedMessageId = message.id,
        decidedAction = action,
        decidedSurface = "android_app",
      )
    }
    dat?.dismissActiveCard()
    sendAckFor(message, action, "android_app")
  }

  fun resend(message: BusMessage) {
    _state.update { it.copy(pendingResend = null) }
    sendDat(message)
  }

  private fun client() = BusClient(serverUrl.trim(), token.trim())

  private fun handleMessage(message: BusMessage) {
    val app = appContext ?: return
    _state.update {
      it.copy(
        lastId = message.id,
        lastMessage = message,
        pendingDecision = message,
        status = "Received #${message.id}; notification posted; sending DAT card",
      )
    }
    prefs(app).edit().putLong(PREF_LAST_ID, message.id).apply()
    showWalkawayNotification(app, message)
    sendDat(message)
    scope.launch {
      runCatching { withContext(Dispatchers.IO) { client().ack(message.id, "received", "android_app") } }
    }
  }

  private fun sendDat(message: BusMessage) {
    val d = dat ?: return
    if (cardedDedupe?.mark(message.id) != true) {
      _state.update { it.copy(status = "Duplicate card suppressed for #${message.id}") }
      return
    }
    _state.update { it.copy(cardMessage = message) }
    val cardBody = buildString {
      append(message.body)
      if (!message.price.isNullOrBlank()) append("\n").append(message.price)
      if (!message.detail.isNullOrBlank()) append("\n").append(message.detail)
    }
    d.sendCard(
      title = message.title,
      body = cardBody,
      imageUrl = message.imageUrl,
      price = message.price,
      detail = message.detail,
      onAction = { action ->
        if (decidedDedupe?.mark(message.id) == true) {
          _state.update {
            it.copy(
              cardMessageId = null,
              cardMessage = null,
              pendingDecision = null,
              pendingResend = null,
              decidedMessageId = message.id,
              decidedAction = action,
              decidedSurface = "glasses",
            )
          }
          sendAckFor(message, action, "glasses_dat")
        }
      },
      onResult = { result ->
        result.onSuccess {
          _state.update {
            it.copy(
              cardMessageId = message.id,
              decidedMessageId = null,
              decidedAction = null,
              decidedSurface = null,
              status = "DAT card sent for #${message.id}",
            )
          }
          sendAckFor(message, "dat_sent", "android_dat")
        }.onFailure { err ->
          // Failed sends must not consume the one-card dedupe slot (WS3).
          cardedDedupe?.forget(message.id)
          _state.update {
            it.copy(
              pendingResend = message,
              status = "DAT card failed: ${err.message}${datFailureHint(err.message)} Use Resend card.",
            )
          }
          sendAckFor(message, "dat_failed", "android_dat")
        }
      }
    )
  }

  private fun sendAckFor(message: BusMessage, action: String, source: String) {
    scope.launch {
      runCatching { withContext(Dispatchers.IO) { client().ack(message.id, action, source) } }
        .onSuccess {
          _state.update { s -> s.copy(status = "Ack sent: $action for #${message.id} via $source") }
        }
        .onFailure {
          _state.update { s -> s.copy(status = "Ack failed: ${it.message}") }
        }
    }
  }

  private fun prefs(context: Context): SharedPreferences =
    context.getSharedPreferences(BUS_PREFS, Context.MODE_PRIVATE)

  private fun prefsIdDedupe(prefs: SharedPreferences, key: String) = IdDedupe(
    load = { prefs.getStringSet(key, emptySet()).orEmpty().mapNotNull { it.toLongOrNull() }.toSet() },
    save = { ids -> prefs.edit().putStringSet(key, ids.map { it.toString() }.toSet()).apply() }
  )

  private const val TAG = "walkaway.poll"
}
