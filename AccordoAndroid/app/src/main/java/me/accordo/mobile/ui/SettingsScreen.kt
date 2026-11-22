// SPDX-License-Identifier: GPL-3.0-or-later
// Settings: the desktop's cards (AccordoDesktop/src/components/settings/) as a list, each
// card one screen — a phone has room for one at a time. Same titles, tints, rows and notes.

package me.accordo.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import me.accordo.mobile.data.Session
import me.accordo.mobile.data.SettingsSection
import me.accordo.mobile.theme.AccordoColors

@Composable
fun SettingsScreen(session: Session) {
    val section = session.settingsSection
    Column(Modifier.fillMaxSize().background(AccordoColors.main)) {
        Row(
            Modifier.fillMaxWidth().background(AccordoColors.topbar).padding(horizontal = 4.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BackButton { session.closeSettings() }
            Text(
                section?.let { short(it) } ?: "Settings",
                style = MaterialTheme.typography.titleMedium,
                color = AccordoColors.textBright,
                modifier = Modifier.padding(start = 4.dp),
            )
        }
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            when (section) {
                null -> SettingsList(session)
                SettingsSection.Account -> AccountCard(session)
                SettingsSection.Encryption -> EncryptionCard(session)
                SettingsSection.Api -> ApiCard(session)
                SettingsSection.Security -> SecurityCard(session)
                SettingsSection.Sessions -> SessionsCard(session)
            }
        }
    }
}

/** The top bar's title: the desktop card's name, shortened to fit a phone. */
private fun short(section: SettingsSection) = when (section) {
    SettingsSection.Account -> "Account"
    SettingsSection.Encryption -> "Chat encryption"
    SettingsSection.Api -> "API access"
    SettingsSection.Security -> "Sign-in security"
    SettingsSection.Sessions -> "Sessions"
}

// The desktop's card tints (icon tile colour), by card.
private val TintAccount = Color(0xFF60A5FA)
private val TintEncryption = Color(0xFF818CF8)
private val TintApi = Color(0xFFF59E0B)
private val TintSecurity = Color(0xFF34D399)
private val TintSessions = Color(0xFFF87171)

@Composable
private fun SettingsList(session: Session) {
    val me = session.me
    // Who you are, first — the account card.
    Row(
        Modifier
            .fillMaxWidth()
            .background(AccordoColors.card, RoundedCornerShape(14.dp))
            .clickable { session.openSettingsSection(SettingsSection.Account) }
            .padding(14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Avatar(url = session.url(me?.avatar), name = me?.name ?: "?", size = 52.dp)
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(me?.name ?: "—", style = MaterialTheme.typography.titleMedium, color = AccordoColors.textBright)
            Text("@${me?.username.orEmpty()}", style = MaterialTheme.typography.bodySmall, color = AccordoColors.textSecondary)
        }
        Icon(Lucide.chevronRight, null, tint = AccordoColors.faint, modifier = Modifier.size(18.dp))
    }

    Column(Modifier.fillMaxWidth().background(AccordoColors.card, RoundedCornerShape(14.dp))) {
        SectionRow(Lucide.keyRound, TintEncryption, "Chat encryption", if (session.hasChatKey()) "Your key is on this phone" else "Not set up") {
            session.openSettingsSection(SettingsSection.Encryption)
        }
        SectionRow(Lucide.shieldCheck, TintSecurity, "Sign-in security", "Two-factor login: ${if (me?.totpEnabled == true) "on" else "off"}") {
            session.openSettingsSection(SettingsSection.Security)
        }
        SectionRow(Lucide.logOut, TintSessions, "Sessions", "Where you're signed in") {
            session.openSettingsSection(SettingsSection.Sessions)
        }
        SectionRow(Lucide.code, TintApi, "API access", "API login: ${if (me?.apiLogin == true) "on" else "off"}") {
            session.openSettingsSection(SettingsSection.Api)
        }
    }
}

@Composable
private fun SectionRow(icon: ImageVector, tint: Color, title: String, subtitle: String, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconTile(icon, tint)
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, color = AccordoColors.text)
            Text(subtitle, style = MaterialTheme.typography.bodySmall, color = AccordoColors.textSecondary)
        }
        Icon(Lucide.chevronRight, null, tint = AccordoColors.faint, modifier = Modifier.size(18.dp))
    }
}

/** The desktop card's tinted icon tile. */
@Composable
private fun IconTile(icon: ImageVector, tint: Color) {
    Box(
        Modifier.size(34.dp).background(tint.copy(alpha = 0.15f), RoundedCornerShape(9.dp)),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, null, tint = tint, modifier = Modifier.size(18.dp))
    }
}

/** A card's heading inside its screen: the tile and the desktop's full title. */
@Composable
private fun CardHeader(icon: ImageVector, tint: Color, title: String) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        IconTile(icon, tint)
        Spacer(Modifier.width(12.dp))
        Text(title, style = MaterialTheme.typography.titleMedium, color = AccordoColors.textBright, fontWeight = FontWeight.SemiBold)
    }
}

/** The desktop's soft row: label left, value right, on a faint inset. */
@Composable
private fun SoftRow(label: String, value: @Composable () -> Unit) {
    Row(
        Modifier.fillMaxWidth().background(AccordoColors.field, RoundedCornerShape(10.dp)).padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = MaterialTheme.typography.bodyMedium, color = AccordoColors.textSecondary)
        Spacer(Modifier.weight(1f).width(10.dp))
        value()
    }
}

@Composable
private fun SoftRow(label: String, value: String, colour: Color = AccordoColors.textBody, mono: Boolean = false) {
    SoftRow(label) {
        Text(
            value,
            style = MaterialTheme.typography.bodyMedium,
            color = colour,
            fontWeight = FontWeight.SemiBold,
            fontFamily = if (mono) FontFamily.Monospace else null,
        )
    }
}

@Composable
private fun Note(text: String) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = AccordoColors.textSecondary)
}

@Composable
private fun Pill(text: String, colour: Color) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        fontWeight = FontWeight.Bold,
        color = colour,
        modifier = Modifier.background(colour.copy(alpha = 0.15f), RoundedCornerShape(6.dp)).padding(horizontal = 7.dp, vertical = 2.dp),
    )
}

// ── the cards ──

@Composable
private fun AccountCard(session: Session) {
    val me = session.me
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
        Avatar(url = session.url(me?.avatar), name = me?.name ?: "?", size = 96.dp)
        Spacer(Modifier.height(12.dp))
        Text(me?.name ?: "—", style = MaterialTheme.typography.headlineSmall, color = AccordoColors.textBright)
        Text("@${me?.username.orEmpty()}", style = MaterialTheme.typography.bodyMedium, color = AccordoColors.textSecondary)
        Spacer(Modifier.height(6.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            if (me?.isAdmin == true) Pill("ADMIN", AccordoColors.amber)
            when (me?.verified) {
                true -> Pill("Verified", AccordoColors.green)
                false -> Pill("Unverified", AccordoColors.amberText)
                null -> {}
            }
        }
    }

    OutlinedTextField(
        value = session.displayName,
        onValueChange = { session.editDisplayName(it) },
        label = { Text("Display name") },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
    Button(onClick = { session.saveDisplayName() }, enabled = !session.busy, modifier = Modifier.fillMaxWidth()) { Text("Save") }
    session.settingsNotice?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = AccordoColors.green) }
    ErrorLine(session.error)

    me?.email?.takeIf { it.isNotBlank() }?.let { SoftRow("Email", it) }
    SoftRow("Username", "@${me?.username.orEmpty()}")
    SoftRow("Server", session.serverUrl().removePrefix("https://"))
}

@Composable
private fun EncryptionCard(session: Session) {
    val me = session.me
    CardHeader(Lucide.keyRound, TintEncryption, "Chat encryption")
    Row {
        if (session.hasChatKey()) Pill("● ON THIS PHONE", AccordoColors.green) else Pill("NOT SET UP", AccordoColors.amber)
    }
    Note(
        "Private chats and groups are encrypted on your device with your own chat key. Accordo only ever stores " +
            "encrypted messages — and, if you set a chat password, an encrypted backup of your key that only you can open.",
    )
    session.keyFingerprint()?.let { fingerprint ->
        Column(
            Modifier.fillMaxWidth().background(AccordoColors.field, RoundedCornerShape(10.dp)).padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text("Key fingerprint", style = MaterialTheme.typography.labelSmall, color = AccordoColors.faint)
            Text(fingerprint, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, color = AccordoColors.textBody)
            Text("Another device compares this to be sure it's really you.", style = MaterialTheme.typography.labelSmall, color = AccordoColors.faint)
        }
    }
    SoftRow("Chat password", if (me?.hasKeyBackup == true) "Backup saved" else "Not set", if (me?.hasKeyBackup == true) AccordoColors.green else AccordoColors.textSecondary)
    Note(
        "Each account has a key pair. Messages are encrypted for the people in the chat before they leave your device; " +
            "the server relays and stores them without being able to read them. Your private key never leaves the device " +
            "unencrypted: the backup is sealed with your chat password.",
    )
}

@Composable
private fun ApiCard(session: Session) {
    val me = session.me
    CardHeader(Lucide.code, TintApi, "API Access & Developer Keys")
    SoftRow("Enable API Login", if (me?.apiLogin == true) "On" else "Off", if (me?.apiLogin == true) AccordoColors.green else AccordoColors.textSecondary)
    SoftRow("Active API Key", if (me?.apiKey != null) "sk_•••••••• (hidden)" else "none yet", mono = true)
    Note("API-key sessions can use your account but can't change your password-protected settings: wallets, E2EE keys or this key.")
}

@Composable
private fun SecurityCard(session: Session) {
    val me = session.me
    CardHeader(Lucide.shieldCheck, TintSecurity, "Sign-in Security")
    Note("Add a code from your phone to every sign-in, so a leaked password alone is not enough.")
    SoftRow("Two-factor login", if (me?.totpEnabled == true) "On" else "Off", if (me?.totpEnabled == true) AccordoColors.green else AccordoColors.textSecondary)
    val left = me?.recoveryCodesLeft ?: 0
    SoftRow("Recovery codes", "$left left", if (left == 0) AccordoColors.amber else AccordoColors.textBody)
}

@Composable
private fun SessionsCard(session: Session) {
    val me = session.me
    var confirmLogout by rememberSaveable { mutableStateOf(false) }
    CardHeader(Lucide.logOut, TintSessions, "Session & Account Actions")
    Note("Signed in as @${me?.username.orEmpty()}. Sign out a device you don't recognise, or this one.")
    Text("Where you're signed in", style = MaterialTheme.typography.labelMedium, color = AccordoColors.label, fontWeight = FontWeight.SemiBold)

    val rows = session.deviceSessions
    if (rows == null) {
        Note("Loading…")
    } else {
        for (row in rows) {
            val s = row.session
            val icon = when {
                s.viaApiKey -> Lucide.keyRound
                listOf("Android", "iPhone", "iPad", "app").any { s.device.contains(it) } -> Lucide.smartphone
                else -> Lucide.laptop
            }
            var detail = row.signedIn?.let { "Signed in $it" } ?: ""
            if (!s.current && !row.active.isNullOrBlank()) detail += " · active ${row.active}"
            Row(
                Modifier.fillMaxWidth().background(AccordoColors.field, RoundedCornerShape(10.dp)).padding(horizontal = 14.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(icon, null, tint = AccordoColors.textSecondary, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(
                            s.device + if (s.viaApiKey) " · API key" else "",
                            style = MaterialTheme.typography.bodyMedium,
                            color = AccordoColors.textBright,
                            fontWeight = FontWeight.SemiBold,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        if (s.current) Pill("THIS DEVICE", AccordoColors.green)
                    }
                    if (detail.isNotBlank()) Text(detail, style = MaterialTheme.typography.labelSmall, color = AccordoColors.textSecondary)
                }
                if (!s.current) {
                    TextButton(onClick = { session.endDeviceSession(s.id) }, enabled = session.sessionBusy == null) {
                        Text(if (session.sessionBusy == s.id) "…" else "Sign out")
                    }
                }
            }
        }
        if (rows.count { !it.session.current } > 1) {
            OutlinedButton(onClick = { session.endOtherDeviceSessions() }, enabled = session.sessionBusy == null, modifier = Modifier.fillMaxWidth()) {
                Text(if (session.sessionBusy == "others") "Signing out…" else "Sign out all other devices")
            }
        }
    }
    session.settingsNotice?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = AccordoColors.green) }
    ErrorLine(session.error)
    Note("We keep the device name and roughly when it was last active — not your IP address or location.")

    Spacer(Modifier.height(4.dp))
    Button(
        onClick = { confirmLogout = true },
        colors = ButtonDefaults.buttonColors(containerColor = AccordoColors.red.copy(alpha = 0.15f), contentColor = AccordoColors.red),
        modifier = Modifier.fillMaxWidth(),
    ) { Text("Log Out from Accordo") }

    if (confirmLogout) {
        AlertDialog(
            onDismissRequest = { confirmLogout = false },
            title = { Text("Sign out?") },
            text = { Text("You'll need your password to sign in again on this phone.") },
            confirmButton = { TextButton(onClick = { confirmLogout = false; session.logout() }) { Text("Sign out", color = AccordoColors.red) } },
            dismissButton = { TextButton(onClick = { confirmLogout = false }) { Text("Cancel") } },
            containerColor = AccordoColors.card,
        )
    }
}
