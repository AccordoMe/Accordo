// SPDX-License-Identifier: GPL-3.0-or-later
// The chat list: the desktop's sidebar, one screen — avatar, name, last message, time, plus the
// search field the desktop keeps above it. The account (profile, wallets, sign out) is in the side
// menu: the ☰ button or a swipe from the left edge.

package me.accordo.mobile.ui

import androidx.activity.compose.BackHandler
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import me.accordo.mobile.data.Session
import me.accordo.mobile.model.Room
import me.accordo.mobile.theme.AccordoColors

@Composable
fun ChatsScreen(session: Session) {
    val drawer = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    // Back closes the menu first; with it shut, back leaves the app as usual.
    BackHandler(enabled = drawer.isOpen) { scope.launch { drawer.close() } }

    ModalNavigationDrawer(
        drawerState = drawer,
        drawerContent = { AccountDrawer(session) { scope.launch { drawer.close() } } },
    ) {
        ChatList(session, openMenu = { scope.launch { drawer.open() } })
    }
}

@Composable
private fun ChatList(session: Session, openMenu: () -> Unit) {
    Column(Modifier.fillMaxSize().background(AccordoColors.bgDark)) {
        Row(
            Modifier.fillMaxWidth().background(AccordoColors.topbar).padding(horizontal = 4.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // The account lives in the side menu (a swipe from the left edge opens it too).
            IconButton(onClick = openMenu) {
                Icon(Lucide.menu, contentDescription = "Menu", tint = AccordoColors.text, modifier = Modifier.size(22.dp))
            }
            Text(
                text = "Chats",
                style = MaterialTheme.typography.titleMedium,
                color = AccordoColors.textBright,
                modifier = Modifier.weight(1f).padding(start = 4.dp),
            )
            IconButton(onClick = { session.refresh() }, enabled = !session.busy) {
                Icon(Lucide.refresh, contentDescription = "Refresh", tint = AccordoColors.textSecondary, modifier = Modifier.size(20.dp))
            }
        }

        OutlinedTextField(
            value = session.search,
            onValueChange = { session.searchRooms(it) },
            placeholder = { Text("Search chats") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Text, imeAction = ImeAction.Search),
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
        )

        ErrorLine(session.error, Modifier.padding(horizontal = 16.dp, vertical = 4.dp))

        val shown = session.visibleRooms()
        when {
            session.rooms.isEmpty() -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text(
                    if (session.busy) "Loading…" else "No chats yet",
                    color = AccordoColors.textSecondary,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }

            shown.isEmpty() -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text("Nothing matches “${session.search}”", color = AccordoColors.textSecondary)
            }

            else -> {
                SectionLabel("Chats")
                LazyColumn(Modifier.fillMaxSize()) {
                    items(shown, key = { it.roomKey }) { room ->
                        RoomRow(
                            room = room,
                            avatarUrl = session.url(room.avatar),
                            onOpen = { session.openRoom(room) },
                        )
                        HorizontalDivider(color = AccordoColors.border)
                    }
                }
            }
        }
    }
}

/** One row: avatar, name, last message — the desktop list item, tap to open. */
@Composable
private fun RoomRow(room: Room, avatarUrl: String?, onOpen: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onOpen)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Avatar(url = avatarUrl, name = room.name)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.Center) {
            Text(
                text = room.name,
                color = AccordoColors.text,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(2.dp))
            RoomSubtitle(room.lastMessage, room.chatType)
        }
        room.lastTimestamp?.takeIf { it.isNotBlank() }?.let {
            Text(it.substringAfter('T').take(5), color = AccordoColors.faint, style = MaterialTheme.typography.labelSmall)
        }
    }
}
