package com.example.walkawaybridge

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Typeface
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.core.content.ContextCompat
import com.meta.wearable.dat.core.Wearables
import com.meta.wearable.dat.core.selectors.SpecificDeviceSelector
import com.meta.wearable.dat.core.session.DeviceSession
import com.meta.wearable.dat.core.session.DeviceSessionState
import com.meta.wearable.dat.core.types.Device
import com.meta.wearable.dat.core.types.DeviceIdentifier
import com.meta.wearable.dat.core.types.DeviceSessionError
import com.meta.wearable.dat.core.types.LinkState
import com.meta.wearable.dat.core.types.RegistrationState
import com.meta.wearable.dat.display.Display
import com.meta.wearable.dat.display.addDisplay
import com.meta.wearable.dat.display.removeDisplay
import com.meta.wearable.dat.display.types.DisplayState
import com.meta.wearable.dat.display.views.Alignment
import com.meta.wearable.dat.display.views.ButtonStyle
import com.meta.wearable.dat.display.views.CornerRadius
import com.meta.wearable.dat.display.views.Direction
import com.meta.wearable.dat.display.views.FlexBoxBackground
import com.meta.wearable.dat.display.views.IconName
import com.meta.wearable.dat.display.views.ImageSize
import com.meta.wearable.dat.display.views.TextColor
import com.meta.wearable.dat.display.views.TextStyle
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

private const val TAG = "walkaway.dat"

data class DatStatus(
  val initialized: Boolean = false,
  val bluetoothEnabled: Boolean = true,
  val registration: String = "UNAVAILABLE",
  val deviceCount: Int = 0,
  val connectedDevice: String? = null,
  val lastEvent: String = "DAT not initialized"
)

class WalkawayDat(private val appContext: Context) {
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
  private val _status = MutableStateFlow(DatStatus(bluetoothEnabled = isBluetoothEnabled(appContext)))
  val status: StateFlow<DatStatus> = _status.asStateFlow()

  private val devices = MutableStateFlow<Set<DeviceIdentifier>>(emptySet())
  private val metadata = MutableStateFlow<Map<DeviceIdentifier, Device>>(emptyMap())
  private val metadataJobs = ConcurrentHashMap<DeviceIdentifier, Job>()

  private val sessionLock = Any()
  @Volatile private var session: DeviceSession? = null
  @Volatile private var display: Display? = null
  private var sessionStateJob: Job? = null
  private var sessionErrorJob: Job? = null
  private var displayStateJob: Job? = null
  private var countdownJob: Job? = null
  private var waitConnectJob: Job? = null
  private var pendingDeviceId: DeviceIdentifier? = null
  private var onDisplayReady: ((Display) -> Unit)? = null
  private var onDisplayFailed: ((String) -> Unit)? = null
  /** Fired when a shown card disappears without Approve/Reject (WS3 resend). */
  @Volatile var onCardGone: (() -> Unit)? = null
  /** Fired when a display-capable glasses device transitions to CONNECTED so pending cards auto-retry. */
  @Volatile var onGlassesConnected: (() -> Unit)? = null
  @Volatile private var awaitingAction = false
  private var btReceiverRegistered = false

  private val btStateReceiver = object : BroadcastReceiver() {
    override fun onReceive(context: Context?, intent: Intent?) {
      if (intent?.action != BluetoothAdapter.ACTION_STATE_CHANGED) return
      val state = intent.getIntExtra(BluetoothAdapter.EXTRA_STATE, BluetoothAdapter.ERROR)
      when (state) {
        BluetoothAdapter.STATE_OFF, BluetoothAdapter.STATE_TURNING_OFF -> {
          _status.update {
            it.copy(
              bluetoothEnabled = false,
              connectedDevice = null,
              lastEvent = "Bluetooth is OFF — enable Bluetooth to connect glasses"
            )
          }
        }
        BluetoothAdapter.STATE_ON -> {
          _status.update {
            it.copy(
              bluetoothEnabled = true,
              lastEvent = "Bluetooth turned ON — reconnecting glasses…"
            )
          }
          refreshNow()
        }
      }
    }
  }

  private fun notifyCardGone(reason: String) {
    if (!awaitingAction) return
    awaitingAction = false
    Log.i(TAG, "card gone without decision: $reason")
    setEvent("Card gone without decision ($reason)")
    onCardGone?.invoke()
  }
  private var monitoringStarted = false

  fun initialize(activity: ComponentActivity) {
    ensureBluetoothEnabled(appContext, activity)
    if (!processInitialized.compareAndSet(false, true)) {
      _status.update {
        it.copy(
          initialized = true,
          bluetoothEnabled = isBluetoothEnabled(appContext),
          lastEvent = "Wearables already initialized in this process"
        )
      }
      return
    }
    Wearables.initialize(activity)
    _status.update {
      it.copy(
        initialized = true,
        bluetoothEnabled = isBluetoothEnabled(appContext),
        lastEvent = "Wearables.initialize complete"
      )
    }
    startMonitoring()
    refreshNow()
  }

  fun startRegistration(activity: Activity) {
    if (_status.value.registration == RegistrationState.REGISTERED.name) {
      setEvent("Already registered")
      return
    }
    setEvent("Opening Meta AI registration")
    Wearables.startRegistration(activity)
  }

  fun refreshNow() {
    _status.update { it.copy(bluetoothEnabled = isBluetoothEnabled(appContext)) }
    if (!_status.value.initialized) return
    val identifiers = Wearables.devices.value
    updateDevices(identifiers)
    for (id in identifiers) {
      Wearables.devicesMetadata[id]?.value?.let { updateMetadata(id, it) }
      ensureMetadataWatch(id)
    }
  }

  private fun startMonitoring() {
    if (!btReceiverRegistered) {
      runCatching {
        appContext.registerReceiver(btStateReceiver, IntentFilter(BluetoothAdapter.ACTION_STATE_CHANGED))
        btReceiverRegistered = true
      }
    }
    if (monitoringStarted) return
    monitoringStarted = true
    val handler = CoroutineExceptionHandler { _, error -> setEvent("DAT monitoring failed: ${error.message}") }
    scope.launch(handler) {
      Wearables.registrationState.collect { value ->
        Log.i(TAG, "registrationState=$value")
        _status.update { it.copy(registration = value.name, lastEvent = "registrationState=$value") }
      }
    }
    scope.launch(handler) {
      Wearables.registrationErrorStream.collect { error ->
        Log.e(TAG, "registrationError name=${error.name} description=${error.description}")
        setEvent("registrationError=${error.name}")
      }
    }
    scope.launch(handler) {
      Wearables.devices.collect { updateDevices(it) }
    }
  }

  private fun updateDevices(identifiers: Set<DeviceIdentifier>) {
    devices.value = identifiers
    val removed = metadataJobs.keys.toSet() - identifiers
    removed.forEach { metadataJobs.remove(it)?.cancel() }
    if (removed.isNotEmpty()) {
      metadata.update { current -> current.filterKeys { it !in removed } }
    }
    identifiers.forEach(::ensureMetadataWatch)
    publishStatus("devices=${identifiers.size}")
  }

  private fun ensureMetadataWatch(id: DeviceIdentifier) {
    val existing = metadataJobs[id]
    if (existing != null && existing.isActive) return
    metadataJobs[id] = scope.launch {
      val flow = Wearables.devicesMetadata[id]
      if (flow == null) {
        Log.w(TAG, "no metadata flow for id=$id")
        return@launch
      }
      updateMetadata(id, flow.value)
      flow.collect { updateMetadata(id, it) }
    }
  }

  private fun updateMetadata(id: DeviceIdentifier, device: Device) {
    Log.i(TAG, "device id=$id snapshot=$device")
    val wasConnected = metadata.value[id]?.linkState == LinkState.CONNECTED
    metadata.update { it.toMutableMap().apply { put(id, device) } }
    publishStatus("device ${device.name} link=${device.linkState.name}")
    if (!wasConnected && device.isDisplayCapable() && device.linkState == LinkState.CONNECTED) {
      Log.i(TAG, "Glasses ${device.name} transitioned to CONNECTED; triggering onGlassesConnected")
      onGlassesConnected?.invoke()
    }
  }

  private fun publishStatus(event: String) {
    val btOn = isBluetoothEnabled(appContext)
    val connected = if (btOn) {
      connectedDisplayDevice()?.let { metadata.value[it]?.name ?: it.toString() }
    } else {
      null
    }
    _status.update {
      it.copy(
        bluetoothEnabled = btOn,
        deviceCount = devices.value.size,
        connectedDevice = connected,
        lastEvent = if (!btOn) "Bluetooth is OFF — enable Bluetooth for glasses link" else event
      )
    }
  }

  private fun setEvent(event: String) {
    _status.update { it.copy(bluetoothEnabled = isBluetoothEnabled(appContext), lastEvent = event) }
    Log.i(TAG, event)
  }

  private fun connectedDisplayDevice(): DeviceIdentifier? =
    devices.value.firstOrNull { id ->
      metadata.value[id]?.let { it.isDisplayCapable() && it.linkState == LinkState.CONNECTED } == true
    }

  private fun displayCapableDevice(): DeviceIdentifier? =
    devices.value.firstOrNull { id -> metadata.value[id]?.isDisplayCapable() == true }

  fun cancelPendingWait() {
    waitConnectJob?.cancel()
    waitConnectJob = null
  }

  fun sendCard(
    title: String,
    body: String,
    imageUrl: String? = null,
    price: String? = null,
    detail: String? = null,
    iconBitmap: Bitmap? = null,
    autoApproveSeconds: Int = 5,
    onAction: (String) -> Unit,
    onResult: (Result<Unit>) -> Unit
  ) {
    // Cancel any older in-flight waitConnectJob so stale cards never fire 20s later
    waitConnectJob?.cancel()
    waitConnectJob = null

    if (!isBluetoothEnabled(appContext)) {
      ensureBluetoothEnabled(appContext, null)
    }
    if (!_status.value.initialized) {
      onResult(Result.failure(IllegalStateException("DAT not initialized")))
      return
    }
    val connected = if (isBluetoothEnabled(appContext)) connectedDisplayDevice() else null
    if (connected != null) {
      prepareAndSend(connected, title, body, imageUrl, price, detail, iconBitmap, autoApproveSeconds, onAction, onResult)
      return
    }
    val candidate = displayCapableDevice()
    if (candidate == null) {
      val error = if (!isBluetoothEnabled(appContext)) {
        "Bluetooth is OFF — turn on Bluetooth to send card to glasses"
      } else {
        "No display-capable glasses known to Meta DAT"
      }
      setEvent(error)
      onResult(Result.failure(IllegalStateException(error)))
      return
    }
    val waitMsg = if (!isBluetoothEnabled(appContext)) {
      "Bluetooth is OFF — turn on Bluetooth; waiting for glasses link…"
    } else {
      "Waiting for glasses link to connect"
    }
    setEvent(waitMsg)
    waitConnectJob = scope.launch {
      val ready = withTimeoutOrNull(20_000) {
        metadata.first { isBluetoothEnabled(appContext) && it[candidate]?.linkState == LinkState.CONNECTED }
      } != null
      if (ready) {
        prepareAndSend(candidate, title, body, imageUrl, price, detail, iconBitmap, autoApproveSeconds, onAction, onResult)
      } else {
        val error = if (!isBluetoothEnabled(appContext)) {
          "Bluetooth is OFF — card queued until Bluetooth is turned ON"
        } else {
          "Timed out waiting for glasses CONNECTED state"
        }
        setEvent(error)
        onResult(Result.failure(IllegalStateException(error)))
      }
    }
  }

  // RYAN EXPLICIT MOCK/FALLBACK AUTHORIZATION: "create a gray dark gray rounded corner dart card with lighter gray inlay for the question. The question is going to be this project is under imminent cascading collapse or failure or these Asians are out of control. Say something use more correct terms and then show me a series of four of your little GitHub graphs and then the button is you're gonna this project is heading for imminent failure. Can I step in and take over? And the answers are yes, five second countdown, no, and then watch, but for this specific clip I'm just going to let it count down and auto hit yes."
  private fun composeHudFrame(
    baseBitmap: Bitmap,
    yesLabel: String,
    pulseRedHigh: Boolean,
    approvedFlash: Boolean = false
  ): Bitmap {
    val w = 540
    val h = 360
    val out = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
    val canvas = Canvas(out)
    canvas.drawColor(Color.BLACK)

    val fillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    val strokePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }

    // 1. Outer frame (Threads/Messenger Display spec: 24px viewport margin, 32px radius, #383D47 frame + #7A828E border)
    val outerRect = RectF(18f, 12f, w - 18f, h - 12f)
    fillPaint.color = Color.rgb(56, 61, 71)
    canvas.drawRoundRect(outerRect, 32f, 32f, fillPaint)
    strokePaint.color = Color.rgb(128, 136, 150)
    strokePaint.strokeWidth = 2f
    canvas.drawRoundRect(outerRect, 32f, 32f, strokePaint)

    // 2. Inner modal card surface (#262A32 opaque smoked slate so zero room background bleeds through)
    val modalRect = RectF(26f, 20f, w - 26f, h - 20f)
    fillPaint.color = Color.rgb(38, 42, 50)
    canvas.drawRoundRect(modalRect, 26f, 26f, fillPaint)

    // 3. Elevated question header inlay (#343944 with 22px radius and #565E6D hairline, generous 24px padding)
    val headerRect = RectF(44f, 36f, w - 44f, 102f)
    fillPaint.color = Color.rgb(52, 57, 68)
    canvas.drawRoundRect(headerRect, 20f, 20f, fillPaint)
    strokePaint.color = Color.rgb(92, 100, 115)
    strokePaint.strokeWidth = 1.5f
    canvas.drawRoundRect(headerRect, 20f, 20f, strokePaint)

    // Gold/amber status dot + large 24px bold question text (Threads headline scale)
    fillPaint.color = Color.rgb(245, 166, 35)
    canvas.drawCircle(68f, 69f, 7f, fillPaint)

    val titlePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
      color = Color.WHITE
      textSize = 23.5f
      typeface = Typeface.create(Typeface.SANS_SERIF, Typeface.BOLD)
      textAlign = Paint.Align.LEFT
    }
    val titleY = headerRect.centerY() - (titlePaint.descent() + titlePaint.ascent()) / 2f
    canvas.drawText("Cascading failure — Take over?", 86f, titleY, titlePaint)

    // 4. Worktree diff rows (3 clean rows with generous 36px vertical rhythm & 20px bold type so it never blurs together)
    data class WtRow(val name: String, val added: String, val removed: String, val blocks: IntArray)
    val rows = listOf(
      WtRow("wt-auth", "+842", "-619", intArrayOf(1, 1, 1, -1, -1)),
      WtRow("wt-core", "+514", "-498", intArrayOf(1, 1, -1, -1, -1)),
      WtRow("main", "+42", "-380", intArrayOf(1, -1, -1, -1, -1))
    )
    val monoPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
      textSize = 20f
      typeface = Typeface.create(Typeface.MONOSPACE, Typeface.BOLD)
      textAlign = Paint.Align.LEFT
    }
    rows.forEachIndexed { idx, row ->
      val ry = 136f + idx * 38f
      monoPaint.color = Color.rgb(230, 236, 244)
      canvas.drawText(row.name, 52f, ry, monoPaint)
      monoPaint.color = Color.rgb(63, 212, 107)
      canvas.drawText(row.added, 178f, ry, monoPaint)
      monoPaint.color = Color.rgb(248, 89, 89)
      canvas.drawText(row.removed, 258f, ry, monoPaint)

      for (b in 0 until 5) {
        val bx = 344f + b * 29f
        val by = ry - 16f
        val bRect = RectF(bx, by, bx + 23f, by + 18f)
        fillPaint.color = when (row.blocks[b]) {
          1 -> Color.rgb(46, 184, 87)
          -1 -> Color.rgb(229, 62, 62)
          else -> Color.rgb(64, 70, 82)
        }
        canvas.drawRoundRect(bRect, 5f, 5f, fillPaint)
      }
    }

    // 5. Bottom action pill row (Threads/Messenger pill geometry: 54px height, 27px radius, crisp 2.5px focus ring)
    val by0 = 256f
    val bh = 54f
    val radius = 27f
    val btnTextPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
      color = Color.WHITE
      textSize = 20f
      typeface = Typeface.create(Typeface.SANS_SERIF, Typeface.BOLD)
      textAlign = Paint.Align.CENTER
    }
    val textY = by0 + bh / 2f - (btnTextPaint.descent() + btnTextPaint.ascent()) / 2f

    // Left button: Yes (Ns) — Focused pill with bright white Threads focus ring
    val yesRect = RectF(44f, by0, 198f, by0 + bh)
    fillPaint.color = if (approvedFlash) Color.rgb(35, 165, 90) else Color.rgb(24, 119, 242)
    canvas.drawRoundRect(yesRect, radius, radius, fillPaint)
    strokePaint.color = Color.WHITE
    strokePaint.strokeWidth = 3f
    canvas.drawRoundRect(yesRect, radius, radius, strokePaint)
    canvas.drawText(yesLabel, yesRect.centerX(), textY, btnTextPaint)

    // Center button: No — Muted crimson Threads destructive pill (#7D242B fill + #F87171 border)
    val noRect = RectF(212f, by0, 336f, by0 + bh)
    fillPaint.color = if (pulseRedHigh) Color.rgb(158, 38, 46) else Color.rgb(88, 28, 34)
    canvas.drawRoundRect(noRect, radius, radius, fillPaint)
    strokePaint.color = if (pulseRedHigh) Color.rgb(252, 142, 142) else Color.rgb(168, 72, 78)
    strokePaint.strokeWidth = if (pulseRedHigh) 2.5f else 1.5f
    canvas.drawRoundRect(noRect, radius, radius, strokePaint)
    btnTextPaint.color = Color.WHITE
    canvas.drawText("No", noRect.centerX(), textY, btnTextPaint)

    // Right button: ✦ Watch — Charcoal pill (#3A3F4B) + gold star & border
    val watchRect = RectF(350f, by0, w - 44f, by0 + bh)
    fillPaint.color = Color.rgb(58, 63, 75)
    canvas.drawRoundRect(watchRect, radius, radius, fillPaint)
    strokePaint.color = Color.rgb(245, 194, 66)
    strokePaint.strokeWidth = 2f
    canvas.drawRoundRect(watchRect, radius, radius, strokePaint)

    val cx = 378f
    val cy = by0 + bh / 2f
    val rOut = 9.5f
    val rIn = 3.5f
    val star = Path().apply {
      moveTo(cx, cy - rOut)
      lineTo(cx + rIn, cy - rIn)
      lineTo(cx + rOut, cy)
      lineTo(cx + rIn, cy + rIn)
      lineTo(cx, cy + rOut)
      lineTo(cx - rIn, cy + rIn)
      lineTo(cx - rOut, cy)
      lineTo(cx - rIn, cy - rIn)
      close()
    }
    fillPaint.color = Color.rgb(255, 215, 80)
    canvas.drawPath(star, fillPaint)

    btnTextPaint.color = Color.rgb(255, 228, 132)
    canvas.drawText("Watch", watchRect.centerX() + 12f, textY, btnTextPaint)
    return out
  }

  private suspend fun renderCardContent(
    readyDisplay: Display,
    title: String,
    body: String,
    iconBitmap: Bitmap?,
    approveLabel: String,
    pulseRedHigh: Boolean = true,
    approvedFlash: Boolean = false,
    onAction: (String) -> Unit
  ) {
    val isFullGraphic = iconBitmap != null && iconBitmap.width > 96
    val frameBitmap = if (isFullGraphic && iconBitmap != null) {
      composeHudFrame(iconBitmap, approveLabel, pulseRedHigh, approvedFlash)
    } else {
      iconBitmap
    }
    readyDisplay.sendContent {
      if (isFullGraphic && frameBitmap != null) {
        flexBox(
          gap = 0,
          padding = 20,
          onClick = {
            if (!awaitingAction) return@flexBox
            awaitingAction = false
            countdownJob?.cancel()
            onAction("approve")
            scope.launch { dismissAndPowerOff() }
          }
        ) {
          image(
            bitmap = frameBitmap,
            sizePreset = ImageSize.FILL,
            cornerRadius = CornerRadius.NONE
          )
        }
      } else {
        flexBox(gap = 12, padding = 24) {
          if (frameBitmap != null) {
            image(
              bitmap = frameBitmap,
              sizePreset = ImageSize.ICON,
              cornerRadius = CornerRadius.NONE,
              alignSelf = Alignment.START
            )
          }
          text(title.take(80), style = TextStyle.HEADING)
          if (body.isNotBlank()) {
            text(body.take(160), style = TextStyle.BODY)
          }
          button(approveLabel, ButtonStyle.PRIMARY, IconName.CHECKMARK, onClick = {
            if (!awaitingAction) return@button
            awaitingAction = false
            countdownJob?.cancel()
            onAction("approve")
            scope.launch { dismissAndPowerOff() }
          })
          button("No", ButtonStyle.SECONDARY, IconName.X, onClick = {
            if (!awaitingAction) return@button
            awaitingAction = false
            countdownJob?.cancel()
            onAction("reject")
            scope.launch { dismissAndPowerOff() }
          })
        }
      }
    }
  }

  private fun prepareAndSend(
    deviceId: DeviceIdentifier,
    title: String,
    body: String,
    imageUrl: String? = null,
    price: String? = null,
    detail: String? = null,
    iconBitmap: Bitmap? = null,
    autoApproveSeconds: Int = 5,
    onAction: (String) -> Unit,
    onResult: (Result<Unit>) -> Unit
  ) {
    prepareDisplay(
      deviceId,
      onReady = { readyDisplay ->
        scope.launch {
          countdownJob?.cancel()
          val isFullGraphic = iconBitmap != null && iconBitmap.width > 96
          val initialLabel = if (isFullGraphic && autoApproveSeconds > 0) "Yes (${autoApproveSeconds}s)" else "Yes"
          runCatching {
            // Single flat flexBox on purpose (2026-11-06 black-screen
            // regression): nested flexBox/CARD layout renders black on
            // current glasses firmware and holds the display.
            renderCardContent(readyDisplay, title, body, iconBitmap, initialLabel, pulseRedHigh = true, onAction = onAction)
          }.onSuccess {
            awaitingAction = true
            setEvent("DAT card sent")
            onResult(Result.success(Unit))
            if (isFullGraphic && autoApproveSeconds > 0) {
              countdownJob = scope.launch {
                // Wait 2.2s after initial ACK so the glasses waveguide projector
                // finishes waking up from cold sleep while still showing "Yes (5s)".
                delay(2200)
                for (remaining in (autoApproveSeconds - 1) downTo 1) {
                  if (!awaitingAction) return@launch
                  val pulseHigh = (remaining % 2 == 1)
                  runCatching {
                    renderCardContent(
                      readyDisplay,
                      title,
                      body,
                      iconBitmap,
                      "Yes (${remaining}s)",
                      pulseRedHigh = pulseHigh,
                      onAction = onAction
                    )
                  }
                  delay(1100)
                }
                if (awaitingAction) {
                  awaitingAction = false
                  runCatching {
                    renderCardContent(
                      readyDisplay,
                      title,
                      body,
                      iconBitmap,
                      "✓ Yes",
                      pulseRedHigh = false,
                      approvedFlash = true,
                      onAction = onAction
                    )
                  }
                  delay(650)
                  setEvent("DAT card auto-approved after ${autoApproveSeconds}s")
                  onAction("approve")
                  dismissAndPowerOff()
                }
              }
            } else {
              // Non-graphic prompt ("Are you ready to do the demo?") defaults to No if unanswered
              countdownJob = scope.launch {
                delay(25_000)
                if (awaitingAction) {
                  awaitingAction = false
                  setEvent("DAT card defaulted to No")
                  onAction("reject")
                  dismissAndPowerOff()
                }
              }
            }
          }.onFailure {
            setEvent("DAT card failed: ${it.message}")
            // Never leave a failed display attached — a held display blocks
            // the glasses' own screen timeout (black until released).
            scope.launch { releaseSessionFully() }
            onResult(Result.failure(it))
          }
        }
      },
      onFailed = { error ->
        setEvent(error)
        onResult(Result.failure(IllegalStateException(error)))
      }
    )
  }

  private fun prepareDisplay(
    deviceId: DeviceIdentifier,
    onReady: (Display) -> Unit,
    onFailed: (String) -> Unit
  ) {
    scope.launch {
      val currentSession = synchronized(sessionLock) { session }
      val currentDisplay = synchronized(sessionLock) { display }
      // Always open a fresh session per card (2026-11-06): the WS3
      // both-STARTED reuse kept a bad display alive after the firmware
      // update and black-screened the glasses. Fresh session is the
      // physically verified Nov 4 path.
      if (currentSession != null || currentDisplay != null) {
        Log.i(TAG, "Releasing stale DAT session before opening fresh glasses card")
        releaseSessionFully()
        delay(250)
      }
      onDisplayReady = onReady
      onDisplayFailed = onFailed
      synchronized(sessionLock) { pendingDeviceId = deviceId }
      startSession(deviceId)
    }
  }

  private fun startSession(deviceId: DeviceIdentifier, attempt: Int = 0) {
    scope.launch {
      Log.i(TAG, "createSession id=$deviceId attempt=$attempt device=${metadata.value[deviceId]}")
      Wearables.createSession(SpecificDeviceSelector(deviceId)).fold(
        onSuccess = { newSession ->
          synchronized(sessionLock) { session = newSession }
          sessionStateJob?.cancel()
          sessionStateJob = scope.launch {
            newSession.state.collect { state ->
              Log.i(TAG, "session.state -> $state")
              setEvent("session.state=$state")
              when (state) {
                DeviceSessionState.STARTED -> {
                  if (consumePendingDeviceId(deviceId)) attachDisplay()
                }
                DeviceSessionState.STOPPED -> {
                  synchronized(sessionLock) { pendingDeviceId = null }
                  notifyCardGone("session.state=STOPPED")
                  cleanupDisplay()
                }
                else -> Unit
              }
            }
          }
          sessionErrorJob?.cancel()
          sessionErrorJob = scope.launch {
            newSession.errors.collect { handleSessionError(it) }
          }
          newSession.start()
        },
        onFailure = { error, _ ->
          Log.e(TAG, "createSession FAIL: ${error.description}")
          val alreadyExists = error.description.contains("already exists", ignoreCase = true)
          if (alreadyExists && attempt < 2) {
            scope.launch {
              delay(500L * (attempt + 1))
              synchronized(sessionLock) { pendingDeviceId = deviceId }
              startSession(deviceId, attempt + 1)
            }
          } else {
            synchronized(sessionLock) { pendingDeviceId = null }
            failPendingOnce("createSession failed: ${error.description}")
          }
        }
      )
    }
  }

  private fun attachDisplay() {
    val currentSession = synchronized(sessionLock) { session } ?: run {
      failPendingOnce("addDisplay failed: no session")
      return
    }
    currentSession.addDisplay().fold(
      onSuccess = { newDisplay ->
        synchronized(sessionLock) { display = newDisplay }
        Log.i(TAG, "addDisplay SUCCESS")
        displayStateJob?.cancel()
        displayStateJob = scope.launch {
          newDisplay.state.collect { state ->
            Log.i(TAG, "display.state -> $state")
            setEvent("display.state=$state")
            if (state == DisplayState.STARTED) {
              onDisplayReady?.invoke(newDisplay)
            } else {
              notifyCardGone("display.state=$state")
            }
          }
        }
      },
      onFailure = { error, _ ->
        Log.e(TAG, "addDisplay FAIL: ${error.description}")
        failPendingOnce("addDisplay failed: ${error.description}")
      }
    )
  }

  private fun consumePendingDeviceId(deviceId: DeviceIdentifier): Boolean =
    synchronized(sessionLock) {
      if (pendingDeviceId == deviceId && display == null) {
        pendingDeviceId = null
        true
      } else {
        false
      }
    }

  private fun failPendingOnce(error: String) {
    val callback = onDisplayFailed
    onDisplayFailed = null
    onDisplayReady = null
    callback?.invoke(error)
  }

  private fun handleSessionError(error: DeviceSessionError) {
    Log.e(
      TAG,
      "session error name=${error.name} description=${error.description} " +
        "localized=${error.getLocalizedDescription(appContext)}"
    )
    synchronized(sessionLock) { pendingDeviceId = null }
    notifyCardGone("session error: ${error.name}")
    failPendingOnce("session error: ${error.name}")
    scope.launch { releaseSessionFully() }
  }

  private fun cleanupDisplay() {
    countdownJob?.cancel()
    countdownJob = null
    displayStateJob?.cancel()
    displayStateJob = null
    synchronized(sessionLock) { display = null }
  }

  /** Dismiss the live DAT card because the phone surface decided (Track B sync). */
  fun dismissActiveCard() {
    awaitingAction = false
    scope.launch { dismissAndPowerOff() }
  }

  private suspend fun dismissAndPowerOff() {
    awaitingAction = false
    val currentDisplay = synchronized(sessionLock) { display }
    if (currentDisplay != null) {
      runCatching {
        currentDisplay.sendContent {
          flexBox(gap = 0, padding = 0, background = FlexBoxBackground.NONE) {}
        }
        delay(180)
      }
    }
    releaseSessionFully()
  }

  private suspend fun releaseSessionFully() {
    val currentSession = synchronized(sessionLock) { session }
    val currentDisplay = synchronized(sessionLock) { display }
    cleanupDisplay()
    if (currentSession != null) {
      val result = withTimeoutOrNull(4_000) { currentSession.removeDisplay() }
      if (result == null) {
        Log.w(TAG, "removeDisplay timed out")
      } else {
        result.fold(
          onSuccess = { Log.i(TAG, "removeDisplay ok") },
          onFailure = { error, _ -> Log.w(TAG, "removeDisplay fail: ${error.description}") }
        )
      }
    }
    runCatching { currentDisplay?.stop() }
    sessionStateJob?.cancel()
    sessionStateJob = null
    sessionErrorJob?.cancel()
    sessionErrorJob = null
    val stopped = synchronized(sessionLock) { session.also { session = null } }
    runCatching { stopped?.stop() }
  }

  companion object {
    @Volatile private var instance: WalkawayDat? = null
    private val processInitialized = java.util.concurrent.atomic.AtomicBoolean(false)

    fun getInstance(context: Context): WalkawayDat =
      instance ?: synchronized(this) {
        instance ?: WalkawayDat(context.applicationContext).also { instance = it }
      }

    val PERMISSIONS = arrayOf(
      Manifest.permission.BLUETOOTH,
      Manifest.permission.BLUETOOTH_CONNECT,
      Manifest.permission.INTERNET,
      Manifest.permission.POST_NOTIFICATIONS
    )

    fun permissionsGranted(context: Context): Boolean =
      PERMISSIONS.all {
        ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED
      }

    fun isBluetoothEnabled(context: Context): Boolean {
      val bm = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
      return bm?.adapter?.isEnabled == true
    }

    @SuppressLint("MissingPermission")
    fun ensureBluetoothEnabled(context: Context, activity: Activity? = null): Boolean {
      val bm = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
      val adapter = bm?.adapter ?: return false
      if (adapter.isEnabled) return true
      val hasConnectPerm = ContextCompat.checkSelfPermission(
        context,
        Manifest.permission.BLUETOOTH_CONNECT
      ) == PackageManager.PERMISSION_GRANTED
      if (hasConnectPerm) {
        runCatching {
          @Suppress("DEPRECATION")
          adapter.enable()
        }
        if (!adapter.isEnabled && activity != null) {
          runCatching {
            activity.startActivity(Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE))
          }
        }
      }
      return adapter.isEnabled
    }
  }
}
