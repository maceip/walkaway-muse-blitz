/*
 * Copyright 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

package com.example.walkawaybridge.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext

// Cash App paint applied to Material3 roles: green actions, black/white fields.
private val DarkColorScheme = darkColorScheme(
    primary = CashGreen,
    onPrimary = CashWhite,
    primaryContainer = CashGreenContainerDark,
    onPrimaryContainer = CashWhite,
    secondary = CashWhite,
    onSecondary = CashBlack,
    tertiary = CashGreen,
    background = CashBlack,
    onBackground = CashWhite,
    surface = CashDarkSurface,
    onSurface = CashWhite,
    surfaceVariant = CashDarkVariant,
    onSurfaceVariant = CashGrayMid,
    outline = CashGrayText,
    error = CashError
)

private val LightColorScheme = lightColorScheme(
    primary = CashGreen,
    onPrimary = CashWhite,
    primaryContainer = CashGreenContainerLight,
    onPrimaryContainer = CashBlack,
    secondary = CashBlack,
    onSecondary = CashWhite,
    tertiary = tertiaryLight,
    background = CashWhite,
    onBackground = CashBlack,
    surface = CashWhite,
    onSurface = CashBlack,
    surfaceVariant = CashGrayLight,
    onSurfaceVariant = CashGrayText,
    outline = CashGrayMid,
    error = CashError
)

@Composable
fun WalkawayBridgeTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    // Dynamic color is available on Android 12+
    dynamicColor: Boolean = false,
    content: @Composable () -> Unit
) {
    val colorScheme = when {
        dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
            val context = LocalContext.current
            if (darkTheme) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        }
        darkTheme -> DarkColorScheme
        else -> LightColorScheme
    }

    MaterialTheme(
        colorScheme = colorScheme,
        typography = Typography,
        shapes = WalkawayShapes,
        content = content
    )
}

