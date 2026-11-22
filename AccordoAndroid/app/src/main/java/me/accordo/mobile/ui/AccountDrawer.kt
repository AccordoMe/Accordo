// SPDX-License-Identifier: GPL-3.0-or-later
// The side menu on the chat list: who you are (the avatar and name open the profile), the network
// picker of the desktop's top bar, then where you can go — the desktop's account card, top bar and
// rail folded into one drawer, because a phone has no rail.

package me.accordo.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.draw.clip
import androidx.compose.ui.Alignment
import androidx.compose.runtime.remember
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.DropdownMenu
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.NavigationDrawerItemDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import me.accordo.mobile.data.Session
import me.accordo.mobile.theme.AccordoColors

/** The drawer's content. [close] slides it shut; every action closes it first. */
@Composable
fun AccountDrawer(session: Session, close: () -> Unit) {
    val me = session.me
    var confirmSignOut by rememberSaveable { mutableStateOf(false) }

    ModalDrawerSheet(
        drawerContainerColor = AccordoColors.rail,
        drawerContentColor = AccordoColors.text,
    ) {
        // Who you are (the avatar and name open the profile) and, under it, the network everything
        // on-chain uses — the desktop's top bar, folded into the menu.
        Column(Modifier.fillMaxWidth().background(AccordoColors.topbar).padding(12.dp)) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .clickable { close(); session.openProfile() }
                    .padding(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Avatar(url = session.url(me?.avatar), name = me?.name ?: "?", size = 52.dp)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        text = me?.name ?: "—",
                        style = MaterialTheme.typography.titleMedium,
                        color = AccordoColors.textBright,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        text = "@${me?.username.orEmpty()}",
                        style = MaterialTheme.typography.bodySmall,
                        color = AccordoColors.textSecondary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Icon(Lucide.chevronRight, contentDescription = "Profile", tint = AccordoColors.faint, modifier = Modifier.size(18.dp))
            }
            Spacer(Modifier.height(8.dp))
            NetworkPicker(session)
        }

        Spacer(Modifier.height(8.dp))
        DrawerItem(Lucide.wallet, "Wallets") { close(); session.openWallets() }
        DrawerItem(Lucide.settings, "Settings") { close(); session.openSettings() }

        Spacer(Modifier.weight(1f))
        HorizontalDivider(color = AccordoColors.border)
        DrawerItem(Lucide.logOut, "Sign out", AccordoColors.red) { confirmSignOut = true }
        Text(
            text = session.serverUrl(),
            style = MaterialTheme.typography.labelSmall,
            color = AccordoColors.faint,
            modifier = Modifier.padding(start = 28.dp, end = 16.dp, bottom = 16.dp),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }

    if (confirmSignOut) {
        AlertDialog(
            onDismissRequest = { confirmSignOut = false },
            title = { Text("Sign out?") },
            text = { Text("You'll need your password to sign in again on this phone.") },
            confirmButton = {
                TextButton(onClick = { confirmSignOut = false; close(); session.logout() }) {
                    Text("Sign out", color = AccordoColors.red)
                }
            },
            dismissButton = { TextButton(onClick = { confirmSignOut = false }) { Text("Cancel") } },
            containerColor = AccordoColors.card,
        )
    }
}

/** The network picker: the current one, tap for the list (mainnets, testnets, local), as on the
 *  desktop. The pick is kept on this phone. */
@Composable
private fun NetworkPicker(session: Session) {
    var open by remember { mutableStateOf(false) }
    val current = session.chain
    Box {
        Row(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(10.dp))
                .background(AccordoColors.field)
                .clickable(enabled = session.chains.isNotEmpty()) { open = true }
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Lucide.globe, contentDescription = null, tint = AccordoColors.indigoSoft, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f)) {
                Text("Network", style = MaterialTheme.typography.labelSmall, color = AccordoColors.faint)
                Text(
                    current?.name ?: if (session.chains.isEmpty()) "Loading…" else "Pick a network",
                    style = MaterialTheme.typography.bodyMedium,
                    color = AccordoColors.text,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (current?.testnet == true || current?.local == true) {
                Tag(if (current.local) "LOCAL" else "TESTNET")
                Spacer(Modifier.width(6.dp))
            }
            Icon(Lucide.chevronDown, contentDescription = "Change network", tint = AccordoColors.textSecondary, modifier = Modifier.size(16.dp))
        }
        DropdownMenu(
            expanded = open,
            onDismissRequest = { open = false },
            containerColor = AccordoColors.card,
            modifier = Modifier.heightIn(max = 420.dp),
        ) {
            val groups = listOf(
                "Mainnets" to session.chains.filter { !it.testnet && !it.local },
                "Testnets" to session.chains.filter { it.testnet && !it.local },
                "Local" to session.chains.filter { it.local },
            )
            for ((label, list) in groups) {
                if (list.isEmpty()) continue
                Text(
                    label.uppercase(),
                    style = MaterialTheme.typography.labelSmall,
                    color = AccordoColors.faint,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
                )
                for (chain in list) {
                    val picked = chain.id == session.chainId
                    DropdownMenuItem(
                        text = {
                            Text(
                                chain.name,
                                color = if (picked) AccordoColors.indigoSoft else AccordoColors.text,
                                fontWeight = if (picked) FontWeight.SemiBold else FontWeight.Normal,
                            )
                        },
                        trailingIcon = {
                            Text(chain.nativeSymbol, style = MaterialTheme.typography.labelSmall, color = AccordoColors.faint)
                        },
                        onClick = { open = false; session.selectChain(chain.id) },
                    )
                }
            }
        }
    }
}

@Composable
private fun Tag(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        fontWeight = FontWeight.Bold,
        color = AccordoColors.amberText,
        modifier = Modifier.background(AccordoColors.amber.copy(alpha = 0.15f), RoundedCornerShape(6.dp)).padding(horizontal = 6.dp, vertical = 2.dp),
    )
}

@Composable
private fun DrawerItem(icon: ImageVector, label: String, colour: Color = AccordoColors.text, onClick: () -> Unit) {
    NavigationDrawerItem(
        icon = { Icon(icon, contentDescription = null, modifier = Modifier.size(20.dp)) },
        label = { Text(label) },
        selected = false,
        onClick = onClick,
        colors = NavigationDrawerItemDefaults.colors(
            unselectedContainerColor = Color.Transparent,
            unselectedIconColor = if (colour == AccordoColors.text) AccordoColors.textSecondary else colour,
            unselectedTextColor = colour,
        ),
        modifier = Modifier.padding(NavigationDrawerItemDefaults.ItemPadding),
    )
}
