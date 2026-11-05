package com.example.walkawaybridge

/** The four demo-visible states of the Android bridge (Partner Lane P, P4). */
enum class BridgePhase(val label: String) {
  DISCONNECTED("DISCONNECTED"),
  CONNECTED_IDLE("CONNECTED — WAITING"),
  CARD_ON_GLASSES("CARD ON GLASSES"),
  DECIDED("DECIDED ON GLASSES")
}

/**
 * Pure phase derivation. A live card outranks the last decision; a recorded
 * decision stays visible until the next card goes out.
 */
fun bridgePhase(connected: Boolean, cardMessageId: Long?, decidedMessageId: Long?): BridgePhase =
  when {
    !connected -> BridgePhase.DISCONNECTED
    cardMessageId != null -> BridgePhase.CARD_ON_GLASSES
    decidedMessageId != null -> BridgePhase.DECIDED
    else -> BridgePhase.CONNECTED_IDLE
  }

/**
 * Persistent first-time marker for bus message ids. Backed by caller-provided
 * load/save so the same dedupe survives app restarts via SharedPreferences.
 * [mark] returns true exactly once per id.
 */
class IdDedupe(private val load: () -> Set<Long>, private val save: (Set<Long>) -> Unit) {
  @Synchronized
  fun mark(id: Long): Boolean {
    val current = load()
    if (id in current) return false
    save(current + id)
    return true
  }
}

/** Human hint for DAT eligibility failures; empty for other failures. */
fun datFailureHint(message: String?): String {
  val text = message ?: return ""
  return if (text.contains("SESSION_ENDED_BY_DEVICE") || text.contains("NO_ELIGIBLE_DEVICE")) {
    " (glasses not worn or neural band not connected)"
  } else {
    ""
  }
}
