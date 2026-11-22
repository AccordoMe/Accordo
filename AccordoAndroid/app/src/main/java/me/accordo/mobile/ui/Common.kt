// SPDX-License-Identifier: GPL-3.0-or-later
// Small pieces every screen uses: the error line, a section header, the accent button.

package me.accordo.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.getValue
import androidx.compose.material3.IconButton
import androidx.compose.material3.Icon
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import me.accordo.mobile.theme.AccordoColors

/** One line above the form: the core's message, shown as the desktop shows it. */
@Composable
fun ErrorLine(message: String?, modifier: Modifier = Modifier) {
    if (message.isNullOrBlank()) return
    Text(
        text = message,
        color = AccordoColors.red,
        style = MaterialTheme.typography.bodySmall,
        modifier = modifier
            .fillMaxWidth()
            .background(AccordoColors.field, RoundedCornerShape(8.dp))
            .padding(horizontal = 12.dp, vertical = 8.dp),
    )
}

/** The pale heading the desktop uses above a block ("Chats", "Accounts"). */
@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(
        text = text,
        color = AccordoColors.textSecondary,
        style = MaterialTheme.typography.labelSmall,
        modifier = modifier.padding(horizontal = 16.dp, vertical = 8.dp),
    )
}

/** A room's one-line subtitle: the last message, or what kind of room it is. */
@Composable
fun RoomSubtitle(lastMessage: String?, chatType: String, modifier: Modifier = Modifier) {
    Column(modifier) {
        Text(
            text = lastMessage?.takeIf { it.isNotBlank() } ?: chatType,
            color = AccordoColors.textSecondary,
            style = MaterialTheme.typography.bodySmall,
            maxLines = 1,
        )
    }
}

/** A round avatar: the server's picture when there is one, else the first letter — as the desktop does.
 *  The letter also stays while the picture loads, and when it can't be loaded. */
@Composable
fun Avatar(url: String?, name: String, size: Dp = 40.dp, modifier: Modifier = Modifier) {
    var loaded by remember(url) { mutableStateOf(false) }
    Box(
        modifier = modifier.size(size).clip(CircleShape).background(AccordoColors.field),
        contentAlignment = Alignment.Center,
    ) {
        if (!loaded) {
            Text(
                text = name.take(1).uppercase(),
                color = AccordoColors.indigoSoft,
                style = if (size >= 64.dp) MaterialTheme.typography.headlineMedium else MaterialTheme.typography.titleMedium,
            )
        }
        if (!url.isNullOrBlank()) {
            AsyncImage(
                model = url,
                contentDescription = name,
                contentScale = ContentScale.Crop,
                onSuccess = { loaded = true },
                onError = { loaded = false },
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

/** The arrow at the left of a top bar: back to where you came from. */
@Composable
fun BackButton(onClick: () -> Unit) {
    IconButton(onClick = onClick) {
        Icon(Lucide.arrowLeft, contentDescription = "Back", tint = AccordoColors.text, modifier = Modifier.size(22.dp))
    }
}
