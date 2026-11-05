package com.example.walkawaybridge

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BridgeStateTest {

  @Test
  fun phase_disconnected_whenNotConnected() {
    assertEquals(
      BridgePhase.DISCONNECTED,
      bridgePhase(connected = false, cardMessageId = 9L, decidedMessageId = 8L)
    )
  }

  @Test
  fun phase_cardOnGlasses_whenCardLive() {
    assertEquals(
      BridgePhase.CARD_ON_GLASSES,
      bridgePhase(connected = true, cardMessageId = 9L, decidedMessageId = 8L)
    )
  }

  @Test
  fun phase_decided_whenDecisionRecordedAndNoLiveCard() {
    assertEquals(
      BridgePhase.DECIDED,
      bridgePhase(connected = true, cardMessageId = null, decidedMessageId = 8L)
    )
  }

  @Test
  fun phase_connectedIdle_otherwise() {
    assertEquals(
      BridgePhase.CONNECTED_IDLE,
      bridgePhase(connected = true, cardMessageId = null, decidedMessageId = null)
    )
  }

  @Test
  fun dedupe_firstMarkTrue_secondMarkFalse_andPersists() {
    var stored: Set<Long> = emptySet()
    val dedupe = IdDedupe(load = { stored }, save = { stored = it })
    assertTrue(dedupe.mark(7L))
    assertFalse(dedupe.mark(7L))
    assertTrue(dedupe.mark(8L))
    // A fresh instance over the same persisted set still rejects repeats.
    val afterRestart = IdDedupe(load = { stored }, save = { stored = it })
    assertFalse(afterRestart.mark(7L))
    assertEquals(setOf(7L, 8L), stored)
  }

  @Test
  fun dedupe_forget_allowsRemark_forResend() {
    var stored: Set<Long> = emptySet()
    val dedupe = IdDedupe(load = { stored }, save = { stored = it })
    assertTrue(dedupe.mark(7L))
    assertFalse(dedupe.mark(7L))
    assertTrue(dedupe.forget(7L))
    assertFalse(dedupe.forget(7L))
    assertTrue(dedupe.mark(7L))
    assertEquals(setOf(7L), stored)
  }

  @Test
  fun reuseDatSession_onlyWhenBothStarted() {
    assertTrue(shouldReuseDatSession("STARTED", "STARTED"))
    assertFalse(shouldReuseDatSession("STARTED", "STOPPED"))
    assertFalse(shouldReuseDatSession("STOPPED", "STARTED"))
    assertFalse(shouldReuseDatSession("STARTING", "STARTED"))
    assertFalse(shouldReuseDatSession(null, null))
  }

  @Test
  fun datFailureHint_mapsEligibilityErrors() {
    assertTrue(datFailureHint("session error: SESSION_ENDED_BY_DEVICE").contains("not worn"))
    assertTrue(datFailureHint("NO_ELIGIBLE_DEVICE").contains("not worn"))
    assertEquals("", datFailureHint("createSession failed: boom"))
  }
}
