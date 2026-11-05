package com.example.walkawaybridge

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import org.junit.Rule
import org.junit.Test

/** Panel contract: every phase renders its label; the panel itself carries no decision buttons (phone Approve/Reject live in BridgeScreen under the active card, §9 dual-surface). */
class BridgeStatusPanelTest {

  @get:Rule val composeTestRule = createAndroidComposeRule<ComponentActivity>()

  @Test
  fun allFourPhases_renderLabels_withoutDecisionButtons() {
    composeTestRule.setContent {
      androidx.compose.foundation.layout.Column {
        BridgePhase.entries.forEach { phase ->
          BridgeStatusPanel(
            phase = phase,
            detail = "detail for $phase",
            datLine = "DAT line for $phase"
          )
        }
      }
    }
    BridgePhase.entries.forEach { phase ->
      composeTestRule.onNodeWithText(phase.label).assertExists()
      composeTestRule.onNodeWithText("detail for $phase").assertExists()
    }
    composeTestRule.onNodeWithText("Approve").assertDoesNotExist()
    composeTestRule.onNodeWithText("Reject").assertDoesNotExist()
  }
}
