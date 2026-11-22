// SPDX-License-Identifier: GPL-3.0-or-later
// The feed and the composer: the desktop's chat page without the rail. Avatars on the left, own
// messages on the right, reactions under the bubbles, a pin mark, a thread count — and a long press
// for the actions the desktop keeps in its hover menu.

package me.accordo.mobile.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import me.accordo.mobile.data.Session
import me.accordo.mobile.model.Message
import me.accordo.mobile.theme.AccordoColors

/** The emoji the desktop's quick row offers. */
private val QUICK = listOf("👍", "❤️", "😂", "😮", "😢")

@Composable
fun ChatScreen(session: Session) {
    val room = session.room ?: return
    var draft by rememberSaveable(room.roomKey) { mutableStateOf("") }
    var actionsFor by remember { mutableStateOf<Message?>(null) }
    val feed = rememberLazyListState()

    LaunchedEffect(session.messages.size) {
        if (session.messages.isNotEmpty()) feed.animateScrollToItem(session.messages.lastIndex)
    }

    Column(Modifier.fillMaxSize().background(AccordoColors.main).imePadding()) {
        Row(
            Modifier.fillMaxWidth().background(AccordoColors.topbar).padding(horizontal = 8.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BackButton { session.back() }
            Avatar(url = session.url(room.avatar), name = room.name, size = 34.dp)
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f)) {
                Text(room.name, style = MaterialTheme.typography.titleMedium, color = AccordoColors.textBright)
                Text(
                    text = when (room.chatType) {
                        "dm" -> "Direct message"
                        "group" -> "Group"
                        "private" -> "Private room"
                        else -> "Room"
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = AccordoColors.textSecondary,
                )
            }
            TextButton(onClick = { session.refresh() }) { Text("Refresh") }
        }

        ErrorLine(session.error, Modifier.padding(horizontal = 16.dp, vertical = 6.dp))
        session.keyNotice?.let { notice ->
            Text(
                text = notice,
                color = AccordoColors.muted,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
            )
        }

        LazyColumn(
            state = feed,
            modifier = Modifier.weight(1f).fillMaxWidth(),
            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            items(session.messages, key = { it.id }) { message ->
                MessageBubble(
                    message = message,
                    text = session.text(message),
                    mine = message.senderId != null && message.senderId == session.me?.id,
                    meId = session.me?.id,
                    avatarUrl = session.url(message.senderAvatar),
                    onLongPress = { actionsFor = message },
                )
            }
        }

        HorizontalDivider(color = AccordoColors.border)
        Row(
            Modifier.fillMaxWidth().background(AccordoColors.list).padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.Bottom,
        ) {
            OutlinedTextField(
                value = draft,
                onValueChange = { draft = it },
                placeholder = { Text("Message") },
                maxLines = 4,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            TextButton(
                onClick = {
                    session.send(draft)
                    draft = ""
                },
                enabled = draft.isNotBlank() && !session.busy,
            ) { Text("Send") }
        }
    }

    actionsFor?.let { message ->
        MessageActions(
            message = message,
            onReact = { emoji ->
                session.react(message, emoji)
                actionsFor = null
            },
            onPin = {
                session.togglePin(message)
                actionsFor = null
            },
            onThread = {
                session.openThread(message)
                actionsFor = null
            },
            onDismiss = { actionsFor = null },
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun MessageBubble(
    message: Message,
    text: String,
    mine: Boolean,
    meId: Long?,
    avatarUrl: String?,
    onLongPress: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = if (mine) Arrangement.End else Arrangement.Start,
        verticalAlignment = Alignment.Top,
    ) {
        if (!mine) {
            Avatar(url = avatarUrl, name = message.senderName, size = 30.dp)
            Spacer(Modifier.width(8.dp))
        }
        Column(Modifier.widthIn(max = 300.dp), horizontalAlignment = if (mine) Alignment.End else Alignment.Start) {
            Column(
                Modifier
                    .combinedClickable(onClick = {}, onLongClick = onLongPress)
                    .background(if (mine) AccordoColors.indigoDark else AccordoColors.field, RoundedCornerShape(12.dp))
                    .padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                if (!mine) {
                    Text(
                        text = message.senderName,
                        style = MaterialTheme.typography.labelSmall,
                        color = AccordoColors.indigoSoft,
                    )
                }
                message.isPinned?.takeIf { it }?.let {
                    Text("📌 pinned", style = MaterialTheme.typography.labelSmall, color = AccordoColors.faint)
                }
                Text(text = text, style = MaterialTheme.typography.bodyMedium, color = AccordoColors.text)
                Box(Modifier.fillMaxWidth()) {
                    Text(
                        text = message.clock,
                        style = MaterialTheme.typography.labelSmall,
                        color = AccordoColors.faint,
                        modifier = Modifier.align(Alignment.CenterEnd),
                    )
                }
            }

            // Reactions: what the server sent, counted; the ones we cast are tinted.
            message.reactions?.takeIf { it.isNotEmpty() }?.let { reactions ->
                Row(Modifier.padding(top = 2.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    reactions.forEach { (emoji, ids) ->
                        val mineToo = meId != null && ids.contains(meId)
                        Text(
                            text = "$emoji ${ids.size}",
                            style = MaterialTheme.typography.labelSmall,
                            color = if (mineToo) AccordoColors.indigoSoft else AccordoColors.textSecondary,
                            modifier = Modifier
                                .background(if (mineToo) AccordoColors.field else AccordoColors.list, RoundedCornerShape(8.dp))
                                .padding(horizontal = 6.dp, vertical = 2.dp),
                        )
                    }
                }
            }

            message.threadSummary?.takeIf { it.count > 0 }?.let { thread ->
                Text(
                    text = "💬 ${thread.count} replies — tap ⋯ to open",
                    style = MaterialTheme.typography.labelSmall,
                    color = AccordoColors.indigoSoft,
                    modifier = Modifier.padding(top = 2.dp),
                )
            }
        }
    }
}

/** Long press: the quick emoji row and the two actions a phone can do without a hover menu. */
@Composable
private fun MessageActions(
    message: Message,
    onReact: (String) -> Unit,
    onPin: () -> Unit,
    onThread: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (message.isPinned == true) "Message (pinned)" else "Message") },
        text = {
            Column {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    QUICK.forEach { emoji ->
                        TextButton(onClick = { onReact(emoji) }) { Text(emoji, style = MaterialTheme.typography.titleLarge) }
                    }
                }
                Spacer(Modifier.height(8.dp))
                TextButton(onClick = onPin) { Text(if (message.isPinned == true) "Unpin" else "Pin") }
                TextButton(onClick = onThread) { Text("Open thread") }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Close") } },
    )
}
