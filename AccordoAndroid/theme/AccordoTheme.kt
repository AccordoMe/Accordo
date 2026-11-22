// SPDX-License-Identifier: GPL-3.0-or-later
// GENERATED from AccordoDesktop/src/theme.rs — do not edit by hand.
// Regenerate: python3 AccordoAndroid/scripts/theme_to_kotlin.py
//
// The desktop client's palette, name for name (its names come from the web app's CSS variables),
// so a Compose screen can be compared with the desktop window side by side.

package me.accordo.mobile.theme

import androidx.compose.ui.graphics.Color

/** Colors of the Accordo desktop client. */
object AccordoColors {
    /** --bg-dark */
    val bgDark = Color(0xFF0A0B10)   // BG_DARK
    /** rgba(13, 14, 20, 0.95) */
    val topbar = Color(0xFF0D0E14)   // TOPBAR
    /** sidebar */
    val rail = Color(0xFF111216)   // RAIL
    val list = Color(0xFF0F1015)   // LIST
    val main = Color(0xFF0C0D12)   // MAIN
    /** chain box */
    val field = Color(0xFF121316)   // FIELD
    /** --accent */
    val accent = Color(0xFF5865F2)   // ACCENT
    val indigo = Color(0xFF6366F1)   // INDIGO
    val indigoDark = Color(0xFF4F46E5)   // INDIGO_DARK
    val indigoSoft = Color(0xFFA5B4FC)   // INDIGO_SOFT
    val indigoIcon = Color(0xFF818CF8)   // INDIGO_ICON
    /** --text-primary */
    val text = Color(0xFFE0E2EB)   // TEXT
    val textBright = Color(0xFFF2F3F5)   // TEXT_BRIGHT
    val textBody = Color(0xFFDBDEE1)   // TEXT_BODY
    /** --text-secondary */
    val textSecondary = Color(0xFF8B92A5)   // TEXT_SECONDARY
    val muted = Color(0xFF94A3B8)   // MUTED
    val faint = Color(0xFF64748B)   // FAINT
    val red = Color(0xFFF23F43)   // RED
    val green = Color(0xFF34D399)   // GREEN
    /** ── fonts (Inter, bundled: the web app loads it from Google Fonts) ── */
    val closeRed = Color(0xFFE5484D)   // CLOSE_RED
    val card = Color(0xFF11131D)   // CARD
    val link = Color(0xFF7C84F6)   // LINK
    val sky = Color(0xFF38BDF8)   // SKY
    val amber = Color(0xFFF59E0B)   // AMBER
    val amberText = Color(0xFFFBBF24)   // AMBER_TEXT
    /** / The settings `.card`; `accent` gives the indigo-bordered variant (chat encryption). */
    val dangerText = Color(0xFFF87171)   // DANGER_TEXT
    /** / Page tabs ("Write & Claim", "Incoming Cheques"…): solid indigo when active. */
    val label = Color(0xFFCBD5E1)   // LABEL
    /** --glass-border */
    val border = Color(0x14FFFFFF)   // BORDER
    val borderStrong = Color(0x1FFFFFFF)   // BORDER_STRONG
}
