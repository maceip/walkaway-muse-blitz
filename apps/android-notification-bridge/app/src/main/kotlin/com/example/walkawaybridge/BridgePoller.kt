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
    val deliveryState: String = "IDLE",
    val skippedStaleCount: Int = 0,
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
        if (gone != null && _state.value.decidedMessageId != gone.id && _state.value.pendingDecision?.id == gone.id) {
          cardedDedupe?.forget(gone.id)
          _state.update {
            it.copy(
              cardMessageId = null,
              deliveryState = "FAILED_NEEDS_RESEND",
              pendingResend = gone,
              status = "Card #${gone.id} gone without a decision. Use Resend card.",
            )
          }
        }
      }
      d.onGlassesConnected = {
        val candidate = _state.value.pendingResend ?: _state.value.pendingDecision
        if (
          candidate != null &&
          _state.value.decidedMessageId != candidate.id &&
          _state.value.cardMessageId != candidate.id
        ) {
          Log.i(TAG, "Glasses reconnected; auto-retrying pending card #${candidate.id}")
          cardedDedupe?.forget(candidate.id)
          _state.update {
            it.copy(
              pendingResend = null,
              deliveryState = "SENDING_TO_GLASSES",
              status = "Glasses connected — auto-sending queued card #${candidate.id}…"
            )
          }
          sendDat(candidate)
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
          withContext(Dispatchers.IO) {
            var cursor = _state.value.lastId
            var newest: BusMessage? = null
            var count = 0
            for (step in 0 until 16) {
              val m = client().nextMessage(cursor) ?: break
              if (m.id <= cursor) break
              newest = m
              cursor = m.id
              count++
            }
            newest to (if (count > 1) count - 1 else 0)
          }
        }.onSuccess { (message, skipped) ->
          if (message != null) {
            if (skipped > 0) {
              _state.update { it.copy(skippedStaleCount = it.skippedStaleCount + skipped) }
            }
            handleMessage(message)
          }
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
        deliveryState = "DECIDED",
        decidedMessageId = message.id,
        decidedAction = action,
        decidedSurface = "android_app",
      )
    }
    dat?.dismissActiveCard()
    sendAckFor(message, action, "android_app")
  }

  fun resend(message: BusMessage) {
    cardedDedupe?.forget(message.id)
    _state.update {
      it.copy(
        pendingResend = null,
        deliveryState = "SENDING_TO_GLASSES",
        status = "Retrying card #${message.id} to glasses…"
      )
    }
    sendDat(message)
  }

  /** Clear any stale/stuck local queue state and immediately resync with the latest card on the laptop bus. */
  fun resyncAndRetryLatest() {
    val app = appContext ?: return
    dat?.cancelPendingWait()
    val p = prefs(app)
    p.edit()
      .remove(PREF_CARDED_IDS)
      .putLong(PREF_LAST_ID, 0L)
      .apply()
    _state.update {
      it.copy(
        lastId = 0L,
        cardMessageId = null,
        pendingResend = null,
        deliveryState = "RESYNCING",
        status = "Cleared stale queue state — fetching latest card from laptop…"
      )
    }
    scope.launch {
      runCatching {
        withContext(Dispatchers.IO) {
          var cursor = 0L
          var newest: BusMessage? = null
          for (step in 0 until 32) {
            val m = client().nextMessage(cursor) ?: break
            if (m.id <= cursor) break
            newest = m
            cursor = m.id
          }
          newest
        }
      }.onSuccess { newest ->
        if (newest != null) {
          cardedDedupe?.forget(newest.id)
          handleMessage(newest)
        } else {
          _state.update { it.copy(deliveryState = "IDLE", status = "Queue resynced — no pending cards on laptop bus.") }
        }
      }.onFailure { err ->
        _state.update { it.copy(deliveryState = "FAILED_NEEDS_RESEND", status = "Resync failed: ${err.message}") }
      }
    }
  }

  private fun client() = BusClient(serverUrl.trim(), token.trim())

  private fun handleMessage(message: BusMessage) {
    val app = appContext ?: return
    val btOn = WalkawayDat.isBluetoothEnabled(app)
    _state.update {
      it.copy(
        lastId = message.id,
        lastMessage = message,
        pendingDecision = message,
        pendingResend = null,
        cardMessageId = null,
        deliveryState = if (btOn) "SENDING_TO_GLASSES" else "WAITING_FOR_BLUETOOTH",
        status = if (btOn) {
          "Received #${message.id}; sending DAT card to glasses…"
        } else {
          "Received #${message.id}; Bluetooth is OFF — turn on Bluetooth to auto-send to glasses"
        },
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
    val iconBitmap = message.imageB64?.let { b64 ->
      runCatching {
        val bytes = android.util.Base64.decode(b64, android.util.Base64.DEFAULT)
        android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
      }.getOrNull()
    }
    d.sendCard(
      title = message.title,
      body = cardBody,
      imageUrl = message.imageUrl,
      price = message.price,
      detail = message.detail,
      iconBitmap = iconBitmap,
      autoApproveSeconds = 5,
      onAction = { action ->
        if (_state.value.pendingDecision?.id == message.id && decidedDedupe?.mark(message.id) == true) {
          _state.update {
            it.copy(
              cardMessageId = null,
              cardMessage = null,
              pendingDecision = null,
              pendingResend = null,
              deliveryState = "DECIDED",
              decidedMessageId = message.id,
              decidedAction = action,
              decidedSurface = "glasses",
            )
          }
          sendAckFor(message, action, "glasses_dat")
        }
      },
      onResult = { result ->
        // Ignore late callbacks from superseded cards so they never overwrite the active card
        if (_state.value.pendingDecision?.id != message.id) return@sendCard
        result.onSuccess {
          _state.update {
            it.copy(
              cardMessageId = message.id,
              deliveryState = "LIVE_ON_GLASSES",
              decidedMessageId = null,
              decidedAction = null,
              decidedSurface = null,
              status = "DAT card #${message.id} is LIVE on glasses",
            )
          }
          sendAckFor(message, "dat_sent", "android_dat")
        }.onFailure { err ->
          // Failed sends must not consume the one-card dedupe slot (WS3).
          cardedDedupe?.forget(message.id)
          val btOn = appContext?.let { WalkawayDat.isBluetoothEnabled(it) } ?: true
          _state.update {
            it.copy(
              pendingResend = message,
              deliveryState = if (!btOn) "WAITING_FOR_BLUETOOTH" else "FAILED_NEEDS_RESEND",
              status = "DAT card #${message.id} queued (${err.message}${datFailureHint(err.message)}). Will auto-send when glasses connect.",
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
