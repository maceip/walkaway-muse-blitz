package com.example.walkawaybridge

import android.Manifest
import android.app.Activity
import android.content.Context
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
  val registration: String = "UNAVAILABLE",
  val deviceCount: Int = 0,
  val connectedDevice: String? = null,
  val lastEvent: String = "DAT not initialized"
)

class WalkawayDat(private val appContext: Context) {
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
  private val _status = MutableStateFlow(DatStatus())
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
  private var pendingDeviceId: DeviceIdentifier? = null
  private var onDisplayReady: ((Display) -> Unit)? = null
  private var onDisplayFailed: ((String) -> Unit)? = null
  /** Fired when a shown card disappears without Approve/Reject (WS3 resend). */
  @Volatile var onCardGone: (() -> Unit)? = null
  @Volatile private var awaitingAction = false

  private fun notifyCardGone(reason: String) {
    if (!awaitingAction) return
    awaitingAction = false
    Log.i(TAG, "card gone without decision: $reason")
    setEvent("Card gone without decision ($reason)")
    onCardGone?.invoke()
  }
  private var monitoringStarted = false

  fun initialize(activity: ComponentActivity) {
    if (!processInitialized.compareAndSet(false, true)) {
      _status.update { it.copy(initialized = true, lastEvent = "Wearables already initialized in this process") }
      return
    }
    Wearables.initialize(activity)
    _status.update { it.copy(initialized = true, lastEvent = "Wearables.initialize complete") }
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
    if (!_status.value.initialized) return
    val identifiers = Wearables.devices.value
    updateDevices(identifiers)
    for (id in identifiers) {
      Wearables.devicesMetadata[id]?.value?.let { updateMetadata(id, it) }
      ensureMetadataWatch(id)
    }
  }

  private fun startMonitoring() {
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
    metadata.update { it.toMutableMap().apply { put(id, device) } }
    publishStatus("device ${device.name} link=${device.linkState.name}")
  }

  private fun publishStatus(event: String) {
    val connected = connectedDisplayDevice()?.let { metadata.value[it]?.name ?: it.toString() }
    _status.update {
      it.copy(
        deviceCount = devices.value.size,
        connectedDevice = connected,
        lastEvent = event
      )
    }
  }

  private fun setEvent(event: String) {
    _status.update { it.copy(lastEvent = event) }
    Log.i(TAG, event)
  }

  private fun connectedDisplayDevice(): DeviceIdentifier? =
    devices.value.firstOrNull { id ->
      metadata.value[id]?.let { it.isDisplayCapable() && it.linkState == LinkState.CONNECTED } == true
    }

  private fun displayCapableDevice(): DeviceIdentifier? =
    devices.value.firstOrNull { id -> metadata.value[id]?.isDisplayCapable() == true }

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
    if (!_status.value.initialized) {
      onResult(Result.failure(IllegalStateException("DAT not initialized")))
      return
    }
    val connected = connectedDisplayDevice()
    if (connected != null) {
      prepareAndSend(connected, title, body, imageUrl, price, detail, iconBitmap, autoApproveSeconds, onAction, onResult)
      return
    }
    val candidate = displayCapableDevice()
    if (candidate == null) {
      val error = "No display-capable glasses known to Meta DAT"
      setEvent(error)
      onResult(Result.failure(IllegalStateException(error)))
      return
    }
    setEvent("Waiting for glasses link to connect")
    scope.launch {
      val ready = withTimeoutOrNull(20_000) {
        metadata.first { it[candidate]?.linkState == LinkState.CONNECTED }
      } != null
      if (ready) {
        prepareAndSend(candidate, title, body, imageUrl, price, detail, iconBitmap, autoApproveSeconds, onAction, onResult)
      } else {
        val error = "Timed out waiting for glasses CONNECTED state"
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
    val w = baseBitmap.width
    val h = baseBitmap.height
    val out = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
    val canvas = Canvas(out)
    canvas.drawColor(Color.BLACK)
    canvas.drawBitmap(baseBitmap, 0f, 0f, null)

    val densityScale = w / 540f
    val by0 = 248f * densityScale
    val bh = 52f * densityScale
    val radius = 26f * densityScale

    val fillPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
    val strokePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    val textPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
      color = Color.WHITE
      textSize = 18f * densityScale
      typeface = Typeface.create(Typeface.SANS_SERIF, Typeface.BOLD)
      textAlign = Paint.Align.CENTER
    }

    // Clear button strip region to the solid inner modal dark-gray (#24242B) so zero room background bleeds through
    fillPaint.color = Color.rgb(36, 36, 43)
    canvas.drawRect(36f * densityScale, (by0 - 4f * densityScale), w - 36f * densityScale, (by0 + bh + 4f * densityScale), fillPaint)

    // 1. Left button: Yes (Ns)
    val yesRect = RectF(40f * densityScale, by0, 202f * densityScale, by0 + bh)
    fillPaint.color = if (approvedFlash) Color.rgb(46, 170, 70) else Color.rgb(10, 132, 255)
    canvas.drawRoundRect(yesRect, radius, radius, fillPaint)
    strokePaint.color = if (approvedFlash) Color.rgb(120, 240, 140) else Color.rgb(120, 195, 255)
    strokePaint.strokeWidth = 2.5f * densityScale
    canvas.drawRoundRect(yesRect, radius, radius, strokePaint)
    val textY = by0 + bh / 2f - (textPaint.descent() + textPaint.ascent()) / 2f
    canvas.drawText(yesLabel, yesRect.centerX(), textY, textPaint)

    // 2. Center button: No (pulsating red)
    val noRect = RectF(214f * densityScale, by0, 344f * densityScale, by0 + bh)
    fillPaint.color = if (pulseRedHigh) Color.rgb(230, 36, 36) else Color.rgb(95, 24, 24)
    canvas.drawRoundRect(noRect, radius, radius, fillPaint)
    strokePaint.color = if (pulseRedHigh) Color.rgb(255, 145, 145) else Color.rgb(175, 58, 58)
    strokePaint.strokeWidth = (if (pulseRedHigh) 3.2f else 1.5f) * densityScale
    canvas.drawRoundRect(noRect, radius, radius, strokePaint)
    textPaint.color = if (pulseRedHigh) Color.WHITE else Color.rgb(235, 185, 185)
    canvas.drawText("No", noRect.centerX(), textY, textPaint)

    // 3. Right button: ✦ Watch (solid #3A3A46 inlay surface + golden fairy star)
    val watchRect = RectF(356f * densityScale, by0, w - 40f * densityScale, by0 + bh)
    fillPaint.color = Color.rgb(58, 58, 70)
    canvas.drawRoundRect(watchRect, radius, radius, fillPaint)
    strokePaint.color = Color.rgb(245, 194, 66)
    strokePaint.strokeWidth = 2f * densityScale
    canvas.drawRoundRect(watchRect, radius, radius, strokePaint)

    val cx = 386f * densityScale
    val cy = by0 + bh / 2f
    val rOut = 9f * densityScale
    val rIn = 3.2f * densityScale
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

    textPaint.color = Color.rgb(255, 225, 120)
    canvas.drawText("Watch", watchRect.centerX() + 12f * densityScale, textY, textPaint)
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
        flexBox(gap = 10, padding = 16) {
          if (frameBitmap != null) {
            image(
              bitmap = frameBitmap,
              sizePreset = ImageSize.ICON,
              cornerRadius = CornerRadius.NONE,
              alignSelf = Alignment.START
            )
          }
          text(title.take(80), style = TextStyle.BODY)
          if (body.isNotBlank()) {
            text(body.take(160), style = TextStyle.BODY)
          }
          button(approveLabel, style = ButtonStyle.PRIMARY, onClick = {
            if (!awaitingAction) return@button
            awaitingAction = false
            countdownJob?.cancel()
            onAction("approve")
            scope.launch { dismissAndPowerOff() }
          })
          button("No", style = ButtonStyle.SECONDARY, onClick = {
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
  }
}
