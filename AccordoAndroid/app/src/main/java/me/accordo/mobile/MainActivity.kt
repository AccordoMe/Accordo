// SPDX-License-Identifier: GPL-3.0-or-later
// The one activity: installs the Keystore-backed secret store, then hands over to Compose.

package me.accordo.mobile

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import android.graphics.Color
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import me.accordo.mobile.core.KeystoreSecrets
import me.accordo.mobile.data.Prefs
import me.accordo.mobile.theme.AccordoTheme
import uniffi.accordo_core.installSecretStore
import uniffi.accordo_core.setConfigDir
import uniffi.accordo_core.secretsAvailable

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Sessions and the chat key are written before anything reads them; without this the core
        // says "no secret store installed" and nothing is remembered between launches.
        installSecretStore(KeystoreSecrets(applicationContext))
        Prefs.init(applicationContext)
        // Pins and settings go to the app's own files: Android has no XDG/HOME to fall back on.
        setConfigDir(filesDir.absolutePath)
        check(secretsAvailable()) { "the secret store was not installed" }

        // Dark-only palette: light system-bar icons, transparent scrims.
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
        )
        setContent {
            AccordoTheme {
                App()
            }
        }
    }
}
