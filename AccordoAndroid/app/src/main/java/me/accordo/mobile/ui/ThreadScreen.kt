// SPDX-License-Identifier: GPL-3.0-or-later
// A thread: the message it hangs under, its replies, and a composer that answers inside it — the
// desktop's thread panel, one screen on a phone.

package me.accordo.mobile.ui

import androidx.compose.foundation.background
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import me.accordo.mobile.data.Session
import me.accordo.mobile.theme.AccordoColors

@Composable
fun ThreadScreen(session: Session) {
    val root = session.threadRoot ?: return
    var draft by rememberSaveable(root.id) { mutableStateOf("") }

    Column(Modifier.fillMaxSize().background(AccordoColors.main).imePadding()) {
        Row(
            Modifier.fillMaxWidth().background(AccordoColors.topbar).padding(horizontal = 8.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BackButton { session.closeThread() }
            Column(Modifier.weight(1f)) {
                Text("Thread", style = MaterialTheme.typography.titleMedium, color = AccordoColors.textBright)
                Text(
                    "${session.replies.size} replies",
                    style = MaterialTheme.typography.bodySmall,
                    color = AccordoColors.textSecondary,
                )
            }
        }

        Column(
            Modifier.fillMaxWidth().background(AccordoColors.list).padding(12.dp),
        ) {
            Text(
                text = root.senderName,
                style = MaterialTheme.typography.labelSmall,
                color = AccordoColors.indigoSoft,
            )
            Text(
                text = session.text(root),
                style = MaterialTheme.typography.bodyMedium,
                color = AccordoColors.text,
            )
        }
        HorizontalDivider(color = AccordoColors.border)

        if (session.replies.isEmpty()) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Text("No replies yet", color = AccordoColors.textSecondary)
            }
        } else {
            LazyColumn(
                modifier = Modifier.weight(1f).fillMaxWidth(),
                contentPadding = PaddingValues(horizontal = 12.dp, vertical = 10.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items(session.replies, key = { it.id }) { reply ->
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Top) {
                        Avatar(url = session.url(reply.senderAvatar), name = reply.senderName, size = 28.dp)
                        Spacer(Modifier.width(8.dp))
                        Column(
                            Modifier
                                .background(AccordoColors.field, RoundedCornerShape(10.dp))
                                .padding(horizontal = 10.dp, vertical = 6.dp),
                        ) {
                            Text(reply.senderName, style = MaterialTheme.typography.labelSmall, color = AccordoColors.indigoSoft)
                            Text(session.text(reply), style = MaterialTheme.typography.bodyMedium, color = AccordoColors.text)
                            Text(reply.clock, style = MaterialTheme.typography.labelSmall, color = AccordoColors.faint)
                        }
                    }
                }
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
                placeholder = { Text("Reply in thread") },
                maxLines = 4,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            TextButton(
                onClick = {
                    session.replyInThread(draft)
                    draft = ""
                },
                enabled = draft.isNotBlank() && !session.busy,
            ) { Text("Send") }
        }
    }
}
