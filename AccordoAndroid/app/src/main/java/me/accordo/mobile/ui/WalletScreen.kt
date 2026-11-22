// SPDX-License-Identifier: GPL-3.0-or-later
// Wallets: the master password unlocks the account's keys (PBKDF2 in Rust, never in Kotlin), the
// list shows what the account holds, and one key at a time can be looked at. Signing still happens
// outside the core for now — that is the next step of this stage.

package me.accordo.mobile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import me.accordo.mobile.data.Session
import me.accordo.mobile.theme.AccordoColors

@Composable
fun WalletScreen(session: Session) {
    var password by rememberSaveable { mutableStateOf("") }

    Column(Modifier.fillMaxSize().background(AccordoColors.main).verticalScroll(rememberScrollState())) {
        Row(
            Modifier.fillMaxWidth().background(AccordoColors.topbar).padding(horizontal = 8.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BackButton { session.closeWallets() }
            Column(Modifier.weight(1f)) {
                Text("Wallets", style = MaterialTheme.typography.titleMedium, color = AccordoColors.textBright)
                Text(
                    if (session.walletsOpen) "Unlocked" else "Locked",
                    style = MaterialTheme.typography.bodySmall,
                    color = AccordoColors.textSecondary,
                )
            }
            if (session.walletsOpen) {
                TextButton(onClick = { session.lockWallets() }) { Text("Lock") }
            }
        }

        if (!session.walletsOpen) {
            Column(Modifier.padding(24.dp)) {
                Text(
                    "Your wallet keys are sealed with your master password. Unlocking takes a moment — " +
                        "the key derivation is deliberately slow.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = AccordoColors.textSecondary,
                )
                Spacer(Modifier.height(16.dp))
                OutlinedTextField(
                    value = password,
                    onValueChange = { password = it },
                    label = { Text("Master password") },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(12.dp))
                Button(
                    onClick = { session.unlockWallets(password) },
                    enabled = !session.busy && password.isNotEmpty(),
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(if (session.busy) "Unlocking…" else "Unlock") }
            }
        } else {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (session.wallets.isEmpty()) {
                    Text("No wallets on this account yet", color = AccordoColors.textSecondary)
                }
                session.wallets.forEach { wallet ->
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .background(AccordoColors.field, RoundedCornerShape(12.dp))
                            .padding(14.dp),
                    ) {
                        Text(
                            wallet.name.ifBlank { "Wallet" },
                            style = MaterialTheme.typography.titleSmall,
                            color = AccordoColors.textBright,
                        )
                        Text(
                            wallet.short,
                            fontFamily = FontFamily.Monospace,
                            style = MaterialTheme.typography.bodySmall,
                            color = AccordoColors.textSecondary,
                        )
                        wallet.chain?.let {
                            Text(it, style = MaterialTheme.typography.labelSmall, color = AccordoColors.faint)
                        }
                        TextButton(onClick = { session.revealKey(wallet) }, enabled = !session.busy) {
                            Text("Show key")
                        }
                    }
                    HorizontalDivider(color = AccordoColors.border)
                }
            }
        }

        ErrorLine(session.error, Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
    }

    session.revealedKey?.let { key ->
        AlertDialog(
            onDismissRequest = { session.dismissRevealedKey() },
            title = { Text("Private key") },
            text = {
                Column {
                    Text(
                        "Anyone who sees this can take the funds. It never leaves the screen.",
                        style = MaterialTheme.typography.bodySmall,
                        color = AccordoColors.textSecondary,
                    )
                    Spacer(Modifier.height(10.dp))
                    Text(key, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                }
            },
            confirmButton = { TextButton(onClick = { session.dismissRevealedKey() }) { Text("Hide") } },
        )
    }
}
