package com.example.walkawaybridge.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Shapes
import androidx.compose.ui.unit.dp

/** Cash App geometry: heavily rounded pills/cards, no sharp corners. */
val WalkawayShapes = Shapes(
  extraSmall = RoundedCornerShape(12.dp),
  small = RoundedCornerShape(16.dp),
  medium = RoundedCornerShape(24.dp),
  large = RoundedCornerShape(28.dp),
  extraLarge = RoundedCornerShape(36.dp)
)
