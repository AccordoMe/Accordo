// SPDX-License-Identifier: GPL-3.0-or-later
// Sign-in, sign-up and password recovery, mirroring the desktop's login page: the same three forms,
// the same rules (terms accepted, 8-character password, an access code on invite-only servers, the
// waitlist when sign-ups are paused), one column for a phone.

package me.accordo.mobile.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
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
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import me.accordo.mobile.data.AuthMode
import me.accordo.mobile.data.Session
import me.accordo.mobile.theme.AccordoColors

@Composable
fun LoginScreen(session: Session) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text("Accordo", style = MaterialTheme.typography.headlineMedium, color = AccordoColors.textBright)
        Text(
            text = when (session.mode) {
                AuthMode.SignIn -> "Sign in to your account"
                AuthMode.Register -> "Create an account"
                AuthMode.Forgot -> "Get back into your account"
            },
            style = MaterialTheme.typography.bodyMedium,
            color = AccordoColors.textSecondary,
            modifier = Modifier.padding(top = 4.dp, bottom = 24.dp),
        )

        when (session.mode) {
            AuthMode.SignIn -> SignInForm(session)
            AuthMode.Register -> RegisterForm(session)
            AuthMode.Forgot -> ForgotForm(session)
        }
    }

    session.recoveryCodes?.let { codes -> RecoveryCodesDialog(codes) { session.dismissRecoveryCodes() } }
}

@Composable
private fun SignInForm(session: Session) {
    var server by rememberSaveable { mutableStateOf("https://accordo.me") }
    var login by rememberSaveable { mutableStateOf("") }
    var password by rememberSaveable { mutableStateOf("") }
    var otp by rememberSaveable { mutableStateOf("") }

    Field(server, { server = it; session.useServer(it) }, "Server", KeyboardType.Uri)
    Field(login, { login = it }, "Email or username", KeyboardType.Email)
    Field(password, { password = it }, "Password", KeyboardType.Password, secret = true)

    // Only when the server answered 403 `two_factor_required`: 2FA accounts, not everyone.
    if (session.needOtp) {
        Field(otp, { otp = it }, "Code from your authenticator app", KeyboardType.NumberPassword)
    }

    ErrorLine(session.error, Modifier.padding(top = 4.dp))
    NoticeLine(session.notice)

    Spacer(Modifier.height(20.dp))
    Button(
        onClick = { session.signIn(login, password, otp.ifBlank { null }) },
        enabled = !session.busy && login.isNotBlank() && password.isNotEmpty() && (!session.needOtp || otp.isNotBlank()),
        modifier = Modifier.fillMaxWidth(),
    ) {
        if (session.busy) Busy() else Text(if (session.needOtp) "Verify and sign in" else "Sign in")
    }

    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        TextButton(onClick = { session.openRegister() }, enabled = !session.busy) { Text("Sign up") }
        TextButton(onClick = { session.openForgot() }, enabled = !session.busy) { Text("Forgot password?") }
    }
}

@Composable
private fun RegisterForm(session: Session) {
    var email by rememberSaveable { mutableStateOf("") }
    var username by rememberSaveable { mutableStateOf("") }
    var password by rememberSaveable { mutableStateOf("") }
    var confirm by rememberSaveable { mutableStateOf("") }
    var accessCode by rememberSaveable { mutableStateOf("") }
    var agreed by rememberSaveable { mutableStateOf(false) }
    var waitlistEmail by rememberSaveable { mutableStateOf("") }

    val status = session.signupStatus

    when {
        status == null -> {
            Text("Checking whether sign-ups are open…", color = AccordoColors.textSecondary)
        }

        // Paused (or full): the desktop offers the waitlist instead of the form.
        status.closed -> {
            Text(
                "Sign-ups are paused on this server. Leave your email and you'll get a note when a spot opens.",
                color = AccordoColors.text,
                style = MaterialTheme.typography.bodyMedium,
            )
            Spacer(Modifier.height(12.dp))
            Field(waitlistEmail, { waitlistEmail = it }, "Email", KeyboardType.Email)
            Button(
                onClick = { session.joinWaitlist(waitlistEmail) },
                enabled = !session.busy && waitlistEmail.contains('@'),
                modifier = Modifier.fillMaxWidth(),
            ) { if (session.busy) Busy() else Text("Join the waitlist") }
        }

        else -> {
            if (status.inviteOnly) {
                Text(
                    "This server is invite-only: your access code stands in for the email.",
                    color = AccordoColors.textSecondary,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(bottom = 12.dp),
                )
                Field(accessCode, { accessCode = it }, "Access code", KeyboardType.Text)
            } else {
                Field(email, { email = it }, "Email", KeyboardType.Email)
            }
            Field(username, { username = it }, "Username", KeyboardType.Text)
            Field(password, { password = it }, "Password (8+ characters)", KeyboardType.Password, secret = true)
            Field(confirm, { confirm = it }, "Repeat the password", KeyboardType.Password, secret = true)

            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(bottom = 8.dp)) {
                Checkbox(checked = agreed, onCheckedChange = { agreed = it })
                Text(
                    "I accept the Terms of Service and Privacy Policy (${session.termsVersion()})",
                    style = MaterialTheme.typography.bodySmall,
                    color = AccordoColors.textSecondary,
                )
            }

            Spacer(Modifier.height(12.dp))
            Button(
                onClick = { session.register(email, username, password, confirm, accessCode) },
                enabled = !session.busy && agreed && username.isNotBlank() && password.isNotEmpty(),
                modifier = Modifier.fillMaxWidth(),
            ) { if (session.busy) Busy() else Text("Create account") }
        }
    }

    ErrorLine(session.error)
    NoticeLine(session.notice)
    TextButton(onClick = { session.showSignIn() }, enabled = !session.busy) { Text("← Back to sign in") }
}

@Composable
private fun ForgotForm(session: Session) {
    var login by rememberSaveable { mutableStateOf("") }
    var code by rememberSaveable { mutableStateOf("") }
    var password by rememberSaveable { mutableStateOf("") }
    var useRecovery by rememberSaveable { mutableStateOf(false) }

    Field(login, { login = it }, "Email or username", KeyboardType.Email)

    if (!useRecovery) {
        TextButton(onClick = { session.sendResetCode(login) }, enabled = !session.busy && login.contains('@')) {
            Text("Email me a 6-digit code")
        }
    }

    Field(code, { code = it }, if (useRecovery) "Recovery code" else "Code from the email", KeyboardType.Text)
    Field(password, { password = it }, "New password (8+ characters)", KeyboardType.Password, secret = true)

    TextButton(onClick = { useRecovery = !useRecovery }) {
        Text(if (useRecovery) "Use the code from the email instead" else "I have a recovery code")
    }

    ErrorLine(session.error)
    NoticeLine(session.notice)

    Spacer(Modifier.height(12.dp))
    Button(
        onClick = {
            if (useRecovery) session.recoverPassword(login, code, password) else session.resetPassword(login, code, password)
        },
        enabled = !session.busy && login.isNotBlank() && code.isNotBlank() && password.isNotEmpty(),
        modifier = Modifier.fillMaxWidth(),
    ) { if (session.busy) Busy() else Text("Change password") }

    TextButton(onClick = { session.showSignIn() }, enabled = !session.busy) { Text("← Back to sign in") }
}

/** Recovery codes are shown once, right after an email-less sign-up: the only way back in later. */
@Composable
private fun RecoveryCodesDialog(codes: List<String>, onDone: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDone,
        title = { Text("Your recovery codes") },
        text = {
            Column {
                Text(
                    "Write them down now — they are shown once and are the only way into this account " +
                        "if you lose the password.",
                    style = MaterialTheme.typography.bodySmall,
                    color = AccordoColors.textSecondary,
                )
                Spacer(Modifier.height(12.dp))
                codes.forEach {
                    Text(it, fontFamily = FontFamily.Monospace, color = AccordoColors.indigoSoft)
                }
            }
        },
        confirmButton = { TextButton(onClick = onDone) { Text("I saved them") } },
    )
}

@Composable
private fun Busy() {
    // Square: with only a height the indicator keeps its 40dp width and the circle is cut off.
    CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp, color = AccordoColors.textBright)
}

@Composable
private fun NoticeLine(text: String?) {
    if (text.isNullOrBlank()) return
    Text(
        text = text,
        color = AccordoColors.indigoSoft,
        style = MaterialTheme.typography.bodySmall,
        modifier = Modifier.padding(top = 8.dp),
    )
}

@Composable
private fun Field(
    value: String,
    onChange: (String) -> Unit,
    label: String,
    keyboard: KeyboardType,
    secret: Boolean = false,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None,
        keyboardOptions = KeyboardOptions(keyboardType = keyboard, imeAction = ImeAction.Next),
        modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp),
    )
}
