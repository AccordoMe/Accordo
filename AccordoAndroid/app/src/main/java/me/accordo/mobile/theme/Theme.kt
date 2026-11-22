// SPDX-License-Identifier: GPL-3.0-or-later
// The Compose theme: the desktop's palette wired into Material 3, so the two clients read the same.
// Colors come from AccordoTheme.kt, which is generated from AccordoDesktop/src/theme.rs.

package me.accordo.mobile.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable

/** The desktop is dark-only (`--bg-dark`), so the phone is too: no light variant to drift from. */
private val AccordoDark = darkColorScheme(
    primary = AccordoColors.accent,
    onPrimary = AccordoColors.textBright,
    primaryContainer = AccordoColors.indigoDark,
    secondary = AccordoColors.indigoIcon,
    onSecondary = AccordoColors.textBright,
    background = AccordoColors.main,
    onBackground = AccordoColors.text,
    surface = AccordoColors.list,
    onSurface = AccordoColors.text,
    surfaceVariant = AccordoColors.field,
    onSurfaceVariant = AccordoColors.textSecondary,
    outline = AccordoColors.border,
    error = AccordoColors.red,
    onError = AccordoColors.textBright,
)

@Composable
fun AccordoTheme(content: @Composable () -> Unit) {
    // isSystemInDarkTheme() is read on purpose: when a light theme is added it starts here.
    val dark = isSystemInDarkTheme() || true
    MaterialTheme(colorScheme = if (dark) AccordoDark else AccordoDark, content = content)
}
