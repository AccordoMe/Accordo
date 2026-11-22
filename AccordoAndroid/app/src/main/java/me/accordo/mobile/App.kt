// SPDX-License-Identifier: GPL-3.0-or-later
// Which screen is on top: sign-in → chat list → chat. The desktop's rail/list/feed order, one level
// at a time, because a phone has one column instead of three.

package me.accordo.mobile

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.lifecycle.viewmodel.compose.viewModel
import me.accordo.mobile.data.Session
import me.accordo.mobile.data.Stage
import me.accordo.mobile.theme.AccordoColors
import me.accordo.mobile.ui.ChatScreen
import me.accordo.mobile.ui.ChatsScreen
import me.accordo.mobile.ui.LoginScreen
import me.accordo.mobile.ui.SettingsScreen
import me.accordo.mobile.ui.ThreadScreen
import me.accordo.mobile.ui.WalletScreen

@Composable
fun App(session: Session = viewModel()) {
    // The phone's back gesture: one level up, the same as each screen's arrow. On the chat list and
    // the sign-in screen it is left to the system (the app goes to the background).
    BackHandler(enabled = session.stage in setOf(Stage.Chat, Stage.Thread, Stage.Settings, Stage.Wallets)) {
        when (session.stage) {
            Stage.Thread -> session.closeThread()
            Stage.Settings -> session.closeSettings()
            Stage.Wallets -> session.closeWallets()
            else -> session.back()
        }
    }

    // Edge-to-edge: the content stays clear of the status and navigation bars, the window colour
    // fills behind them.
    Surface(
        modifier = Modifier.fillMaxSize().background(AccordoColors.bgDark).statusBarsPadding().navigationBarsPadding(),
        color = AccordoColors.bgDark,
    ) {
        if (session.restoring) {
            androidx.compose.foundation.layout.Box(Modifier.fillMaxSize(), contentAlignment = androidx.compose.ui.Alignment.Center) {
                androidx.compose.material3.CircularProgressIndicator(color = AccordoColors.indigoSoft)
            }
            return@Surface
        }

        when (session.stage) {
            Stage.Login -> LoginScreen(session)
            Stage.Chats -> ChatsScreen(session)
            Stage.Chat -> ChatScreen(session)
            Stage.Thread -> ThreadScreen(session)
            Stage.Settings -> SettingsScreen(session)
            Stage.Wallets -> WalletScreen(session)
        }
    }
}
