package com.example.walkawaybridge

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * Demo-safe status panel: shows the current [BridgePhase] in headline type
 * plus the live detail lines. The panel has no decision controls — Hermes
 * decisions count only from physical glasses taps (`source=glasses_dat`),
 * so the phone renders state and never offers Approve/Reject.
 */
@Composable
fun BridgeStatusPanel(phase: BridgePhase, detail: String, datLine: String, modifier: Modifier = Modifier) {
  Card(modifier = modifier.fillMaxWidth()) {
    Column(
      modifier = Modifier.fillMaxWidth().padding(16.dp),
      verticalArrangement = Arrangement.spacedBy(6.dp)
    ) {
      Text(phase.label, style = MaterialTheme.typography.headlineSmall)
      Text(detail, style = MaterialTheme.typography.bodyLarge)
      Text(datLine, style = MaterialTheme.typography.bodyMedium)
    }
  }
}
