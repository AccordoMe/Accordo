// SPDX-License-Identifier: GPL-3.0-or-later
// The app's state: who is signed in, which room is open, what the feed shows. One ViewModel, so a
// rotation doesn't re-fetch and a suspend call always has a live scope. The core does the work —
// this class only turns its calls into Compose state.

package me.accordo.mobile.data

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import me.accordo.mobile.model.AccordoJson
import me.accordo.mobile.model.LoginAnswer
import me.accordo.mobile.model.Message
import me.accordo.mobile.model.Room
import me.accordo.mobile.model.User
import uniffi.accordo_core.Client
import uniffi.accordo_core.Decrypted
import uniffi.accordo_core.DeviceKey
import uniffi.accordo_core.MobileException
import uniffi.accordo_core.RoomKeyring
import uniffi.accordo_core.SocketListener
import uniffi.accordo_core.SocketSubscription
import me.accordo.mobile.model.Answer
import me.accordo.mobile.model.Chain
import me.accordo.mobile.model.SessionRow
import me.accordo.mobile.model.SavedSession
import me.accordo.mobile.model.SignupStatus
import me.accordo.mobile.model.Wallet
import uniffi.accordo_core.absoluteUrl
import uniffi.accordo_core.accountKey
// Aliased: Session has methods of the same names, and a bare `register(...)` would call itself.
import uniffi.accordo_core.forgotPassword as coreForgotPassword
import uniffi.accordo_core.joinWaitlist as coreJoinWaitlist
import uniffi.accordo_core.recoverPassword as coreRecoverPassword
import uniffi.accordo_core.register as coreRegister
import uniffi.accordo_core.registrationStatus as coreRegistrationStatus
import uniffi.accordo_core.resetPassword as coreResetPassword
import uniffi.accordo_core.termsVersion as coreTermsVersion
import uniffi.accordo_core.twoFactorRequired as coreTwoFactorRequired
import uniffi.accordo_core.decryptText
import uniffi.accordo_core.fingerprintOf
import uniffi.accordo_core.forgetSession
import uniffi.accordo_core.loadSession
import uniffi.accordo_core.newCheckVault
import uniffi.accordo_core.openWalletSession
import uniffi.accordo_core.saveCheckVault
import uniffi.accordo_core.savedCheckVault
import uniffi.accordo_core.WalletUnlock
import uniffi.accordo_core.saveSession
import uniffi.accordo_core.loadChatKey
import uniffi.accordo_core.looksEncrypted
import uniffi.accordo_core.previousAccountKey
import uniffi.accordo_core.saveChatKey

/** Which screen is on top. The desktop's rail → list → feed order, one level at a time. */
enum class Stage { Login, Chats, Chat, Thread, Settings, Wallets }

/** The desktop's settings cards, one screen each on the phone, in the desktop's order. */
enum class SettingsSection(val title: String) {
    Account("Account Profile"),
    Encryption("Chat encryption"),
    Api("API Access & Developer Keys"),
    Security("Sign-in Security"),
    Sessions("Session & Account Actions"),
}

/** The three forms the sign-in screen switches between, as on the desktop: `Screen::SignIn`,
 *  `Screen::Register`, `Screen::Forgot`. */
enum class AuthMode { SignIn, Register, Forgot }

class Session : ViewModel() {

    var stage by mutableStateOf(Stage.Login)
        private set
    var busy by mutableStateOf(false)
        private set
    var error by mutableStateOf<String?>(null)
        private set

    var me by mutableStateOf<User?>(null)
        private set
    var rooms by mutableStateOf<List<Room>>(emptyList())
        private set
    var room by mutableStateOf<Room?>(null)
        private set
    var messages by mutableStateOf<List<Message>>(emptyList())
        private set

    /** The open room's keys: the current one first, archived after it. Empty: not sealed. */
    var roomKeys by mutableStateOf<List<String>>(emptyList())
        private set

    /** Why a sealed room can't be read yet (the core's own sentence); null when all is well. */
    var keyNotice by mutableStateOf<String?>(null)
        private set

    // ── the sign-in screen(s) ──
    /** A one-time code is needed (the server answered 403 `two_factor_required`). */
    var mode by mutableStateOf(AuthMode.SignIn)
        private set
    var needOtp by mutableStateOf(false)
        private set
    /** A line under the form: "check your email", "code sent", "the account is on the waitlist". */
    var notice by mutableStateOf<String?>(null)
        private set
    var signupStatus by mutableStateOf<SignupStatus?>(null)
        private set
    /** Shown once, after a sign-up without an email. */
    var recoveryCodes by mutableStateOf<List<String>?>(null)
        private set

    /** The server the sign-in screen is pointed at, remembered between the forms. */
    /** True until the saved session has been checked: the UI shows a loader, not the form. */
    var restoring by mutableStateOf(true)
        private set

    private var server = DEFAULT_SERVER

    /** The login answer as it arrived, kept for the `wallets` array inside it. */
    private var lastAuthJson = "{}"

    /** The chat-list filter (title or last message), as typed. */
    var search by mutableStateOf("")
        private set

    /** The message a thread view hangs under, and its replies. */
    var threadRoot by mutableStateOf<Message?>(null)
        private set
    var replies by mutableStateOf<List<Message>>(emptyList())
        private set

    /** The profile being edited (the signed-in user, refreshed after a save). */
    var displayName by mutableStateOf("")
        private set

    // ── settings ──
    /** The open card; null: the list of them. */
    var settingsSection by mutableStateOf<SettingsSection?>(null)
        private set
    /** "Saved" and the like, under a settings form. */
    var settingsNotice by mutableStateOf<String?>(null)
        private set
    /** Opened from the list (back returns to it) or straight from the side menu (back: the chats). */
    private var sectionFromList = false
    /** Where the account is signed in; null while loading. */
    var deviceSessions by mutableStateOf<List<SessionRow>?>(null)
        private set
    /** The session being signed out ("others": all of them but this one). */
    var sessionBusy by mutableStateOf<String?>(null)
        private set

    // ── networks (the desktop's top-bar picker) ──
    var chains by mutableStateOf<List<Chain>>(emptyList())
        private set
    var chainId by mutableStateOf(Prefs.chainId.orEmpty())
        private set
    val chain: Chain? get() = chains.firstOrNull { it.id == chainId }

    // ── wallets (the master password never leaves Rust) ──
    var wallets by mutableStateOf<List<Wallet>>(emptyList())
        private set
    var walletsOpen by mutableStateOf(false)
        private set
    var revealedKey by mutableStateOf<String?>(null)
        private set

    /** The account's wallets as the server sent them; the vaults are what the core opens. */
    private var walletsJson = "[]"
    private var walletUnlock: WalletUnlock? = null

    private var client: Client? = null
    private var keyring: RoomKeyring? = null
    private var subscription: SocketSubscription? = null

    /** The rooms exactly as the server sent them: opening envelopes needs the untrimmed JSON. */
    private val rawRooms = mutableMapOf<String, String>()

    init {
        restoreSession()
    }

    fun dismissError() {
        error = null
    }

    /**
     * A restart goes straight back into the chat list: the token is in the Keystore, `me()` proves it
     * still works. An expired token (or a wiped store) drops the entry and shows the form.
     */
    private fun restoreSession() {
        viewModelScope.launch {
            try {
                val json = loadSession() ?: return@launch
                val saved = AccordoJson.decodeFromString<SavedSession>(json)
                val api = Client(saved.base, saved.token)
                val meRaw = api.me()
                val user = AccordoJson.decodeFromString<User>(meRaw)
                walletsJson = walletsOf(meRaw)
                server = saved.base
                client = api
                me = user
                keyring = connectChatKey(api, user)
                loadRooms()
                loadChains()
                subscribe(user)
                stage = Stage.Chats
            } catch (e: Exception) {
                runCatching { forgetSession() }
            } finally {
                restoring = false
            }
        }
    }

    /**
     * Signs in, restores (or creates) this device's chat key, and goes to the chat list. With 2FA
     * on, the first answer is 403 `two_factor_required`: the form then shows the code field and
     * calls this again with `otp`.
     */
    fun signIn(login: String, password: String, otp: String?) = carefully {
        val api = Client(server, null)
        val answer = try {
            val raw = api.login(login.trim(), password, otp?.trim()?.ifBlank { null })
            lastAuthJson = raw
            AccordoJson.decodeFromString<LoginAnswer>(raw)
        } catch (e: MobileException.Api) {
            if (e.status?.toInt() == 403 && e.detail == coreTwoFactorRequired()) {
                needOtp = true
                notice = "Enter the code from your authenticator app, or a recovery code"
                return@carefully
            }
            throw e
        }
        finishSignIn(api, answer, answer.token)
    }

    fun useServer(value: String) {
        server = value.trim().ifBlank { DEFAULT_SERVER }
    }

    fun openRegister() {
        mode = AuthMode.Register
        notice = null
        signupStatus = null
        loadSignupStatus()
    }

    fun openForgot() {
        mode = AuthMode.Forgot
        notice = null
    }

    fun showSignIn() {
        mode = AuthMode.SignIn
        needOtp = false
        notice = null
        recoveryCodes = null
    }

    fun dismissNotice() {
        notice = null
    }

    fun dismissRecoveryCodes() {
        recoveryCodes = null
    }

    fun termsVersion(): String = coreTermsVersion()

    /** Whether this server takes sign-ups at all, and whether it wants an access code. */
    fun loadSignupStatus() = carefully {
        signupStatus = AccordoJson.decodeFromString<SignupStatus>(coreRegistrationStatus(server))
    }

    /** Creates the account with a fresh chat key, so the profile is reachable sealed from the start. */
    fun register(email: String, username: String, password: String, confirm: String, accessCode: String) = carefully {
        if (username.trim().length < 3) throw IllegalArgumentException("A username needs at least 3 characters")
        if (password.length < 8) throw IllegalArgumentException("The password needs at least 8 characters")
        if (password != confirm) throw IllegalArgumentException("The passwords don't match")

        val device = DeviceKey.generate()
        val answer = AccordoJson.decodeFromString<LoginAnswer>(
            coreRegister(server, email, username, password, accessCode, device.publicJwk())
        )
        val api = Client(server, answer.token)
        client = api
        me = answer.user
        val account = accountKey(answer.user.id, answer.user.username, server)
        saveChatKey(account, device)
        val ring = RoomKeyring(answer.user.id, account)
        ring.useDeviceKey(device)
        keyring = ring
        saveSession(account, server, answer.token)
        recoveryCodes = answer.recoveryCodes
        stage = Stage.Chats
        mode = AuthMode.SignIn
        loadRooms()
        loadChains()
        subscribe(answer.user)
    }

    fun joinWaitlist(email: String) = carefully {
        AccordoJson.decodeFromString<Answer>(coreJoinWaitlist(server, email))
        notice = "You're on the waitlist — we'll write to $email."
    }

    fun sendResetCode(email: String) = carefully {
        AccordoJson.decodeFromString<Answer>(coreForgotPassword(server, email))
        notice = "A 6-digit code is on its way to $email"
    }

    fun resetPassword(login: String, code: String, password: String) = carefully {
        if (password.length < 8) throw IllegalArgumentException("The password needs at least 8 characters")
        AccordoJson.decodeFromString<Answer>(coreResetPassword(server, login, code, password))
        notice = "Password changed — sign in with the new one"
        mode = AuthMode.SignIn
    }

    fun recoverPassword(login: String, recoveryCode: String, password: String) = carefully {
        if (password.length < 8) throw IllegalArgumentException("The password needs at least 8 characters")
        AccordoJson.decodeFromString<Answer>(coreRecoverPassword(server, login, recoveryCode, password))
        notice = "Password changed — sign in with the new one"
        mode = AuthMode.SignIn
    }

    /** The `wallets` array inside a login answer or a `me` object. */
    private fun walletsOf(json: String): String {
        val root = AccordoJson.parseToJsonElement(json)
        val user = root.jsonObject["user"] ?: root
        return user.jsonObject["wallets"]?.toString() ?: "[]"
    }

    /** Everything both a sign-in and a sign-up end with, the token included. */
    private suspend fun finishSignIn(api: Client, answer: LoginAnswer, token: String) {
        client = api
        me = answer.user
        walletsJson = walletsOf(lastAuthJson)
        val account = accountKey(answer.user.id, answer.user.username, api.base())
        saveSession(account, api.base(), token)
        needOtp = false
        keyring = connectChatKey(api, answer.user)
        loadRooms()
        loadChains()
        subscribe(answer.user)
        stage = Stage.Chats
        mode = AuthMode.SignIn
    }

    fun openRoom(room: Room) = carefully {
        this.room = room
        stage = Stage.Chat
        loadRoomKeys(room)
        loadMessages(room)
    }

    fun back() {
        stage = when (stage) {
            Stage.Chat -> {
                room = null
                messages = emptyList()
                roomKeys = emptyList()
                keyNotice = null
                Stage.Chats
            }
            Stage.Thread -> Stage.Chat
            Stage.Settings -> Stage.Chats
            Stage.Wallets -> if (room != null) Stage.Chat else Stage.Chats
            Stage.Chats -> Stage.Login
            Stage.Login -> Stage.Login
        }
    }

    fun refresh() = carefully {
        val open = room
        if (open != null) {
            loadRoomKeys(open)
            loadMessages(open)
        } else {
            loadRooms()
        }
    }

    fun send(text: String) = carefully {
        val open = room ?: return@carefully
        val api = client ?: return@carefully
        if (text.isBlank()) return@carefully
        // In a sealed room the text is sealed by the core before the request leaves the device.
        api.sendText(open.roomKey, text.trim(), roomKeys.firstOrNull(), null)
        loadMessages(open)
    }

    /** What a message reads as: decrypted when the room's key is here, marked when it isn't. */
    fun text(message: Message): String {
        if (!looksEncrypted(message.content)) {
            return if (message.messageType != "text") "[${message.messageType}]" else message.content
        }
        val keys = roomKeys
        if (keys.isEmpty()) return LOCKED
        return when (val opened = decryptText(message.content, keys)) {
            is Decrypted.Text -> opened.v1
            is Decrypted.Plain -> opened.v1
            Decrypted.Locked -> LOCKED
        }
    }

    fun searchRooms(text: String) {
        search = text
    }

    /** The rooms the filter leaves, newest activity first (the server's order is kept). */
    fun visibleRooms(): List<Room> {
        val q = search.trim().lowercase()
        if (q.isEmpty()) return rooms
        return rooms.filter { it.name.lowercase().contains(q) || (it.lastMessage ?: "").lowercase().contains(q) }
    }

    /** Toggles an emoji on a message; the feed reloads so the counts are the server's. */
    fun react(message: Message, emoji: String) = carefully {
        val api = client ?: return@carefully
        api.react(message.id, emoji)
        refreshFeed()
    }

    fun togglePin(message: Message) = carefully {
        val api = client ?: return@carefully
        api.pin(message.id)
        refreshFeed()
    }

    fun openThread(message: Message) = carefully {
        val api = client ?: return@carefully
        val room = room ?: return@carefully
        threadRoot = message
        replies = AccordoJson.decodeFromString<List<Message>>(api.thread(room.roomKey, message.id, 100L))
        stage = Stage.Thread
    }

    fun closeThread() {
        threadRoot = null
        replies = emptyList()
        stage = Stage.Chat
    }

    /** Answers inside the thread: the same send, with the root set. */
    fun replyInThread(text: String) = carefully {
        val api = client ?: return@carefully
        val room = room ?: return@carefully
        val root = threadRoot ?: return@carefully
        if (text.isBlank()) return@carefully
        // The same SendMessage JSON the web sends; the core seals the content when given a key.
        val payload = buildJsonObject {
            put("room_id", room.roomKey)
            put("thread_root_id", root.id)
            put("content", text.trim())
        }.toString()
        api.sendRaw(payload, roomKeys.firstOrNull())
        replies = AccordoJson.decodeFromString<List<Message>>(api.thread(room.roomKey, root.id, 100L))
    }

    fun openWallets() {
        stage = Stage.Wallets
        wallets = runCatching { AccordoJson.decodeFromString<List<Wallet>>(walletsJson) }.getOrDefault(emptyList())
        walletsOpen = walletUnlock != null && walletUnlock!!.unlocked()
    }

    fun closeWallets() {
        revealedKey = null
        stage = if (room != null) Stage.Chat else Stage.Chats
    }

    /**
     * Unlocks the account's wallets: one PBKDF2 pass per vault, then the keys stay in Rust for
     * `ttlMinutes`. The first time, a password-check vault is made and kept in the Keystore.
     */
    fun unlockWallets(password: String) = carefully {
        val raw = walletsJson
        if (raw == "[]" || raw.isBlank()) throw IllegalStateException("This account has no wallets yet")
        val check = savedCheckVault() ?: newCheckVault(password)
        val opened = openWalletSession(password, check, raw, TTL_MINUTES.toUInt())
        saveCheckVault(opened.checkVault)
        walletUnlock = opened.session
        wallets = AccordoJson.decodeFromString<List<Wallet>>(raw)
        walletsOpen = true
    }

    /** One wallet's private key, as hex — for the signer that is still in Kotlin. */
    fun revealKey(wallet: Wallet) = carefully {
        val unlock = walletUnlock ?: throw IllegalStateException("Unlock the wallets first")
        revealedKey = unlock.reveal(AccordoJson.encodeToString(Wallet.serializer(), wallet))
    }

    fun dismissRevealedKey() {
        revealedKey = null
    }

    fun lockWallets() {
        walletUnlock?.lock()
        walletUnlock = null
        walletsOpen = false
        revealedKey = null
    }

    /** Settings → the list of cards. The user is fetched again so the cards show today's state. */
    fun openSettings() {
        settingsSection = null
        settingsNotice = null
        stage = Stage.Settings
        refreshMe()
    }

    /** One card. [fromList]: back returns to the list; else (the side menu's profile) to the chats. */
    fun openSettingsSection(section: SettingsSection, fromList: Boolean = true) {
        if (section == SettingsSection.Account) displayName = me?.displayName.orEmpty()
        if (section == SettingsSection.Sessions) loadDeviceSessions()
        sectionFromList = fromList
        settingsNotice = null
        settingsSection = section
        if (stage != Stage.Settings) {
            stage = Stage.Settings
            refreshMe()
        }
    }

    /** The side menu's avatar and name: the account card, straight away. */
    fun openProfile() = openSettingsSection(SettingsSection.Account, fromList = false)

    fun closeSettings() {
        settingsNotice = null
        if (settingsSection != null && sectionFromList) {
            settingsSection = null
        } else {
            settingsSection = null
            stage = if (room != null) Stage.Chat else Stage.Chats
        }
    }

    fun editDisplayName(value: String) {
        displayName = value
        settingsNotice = null
    }

    fun saveDisplayName() = carefully {
        val api = client ?: return@carefully
        val user = AccordoJson.decodeFromString<User>(api.updateProfile(displayName))
        me = user
        displayName = user.displayName.orEmpty()
        settingsNotice = "Saved"
    }

    /** The signed-in user again, quietly: a failure keeps what is shown. */
    private fun refreshMe() {
        val api = client ?: return
        viewModelScope.launch {
            runCatching {
                val raw = api.me()
                me = AccordoJson.decodeFromString<User>(raw)
                walletsJson = walletsOf(raw)
            }
        }
    }

    /** Whether this phone holds the chat key (the encryption card's status). */
    fun hasChatKey(): Boolean = keyring != null

    fun loadDeviceSessions() = carefully {
        val api = client ?: return@carefully
        deviceSessions = null
        deviceSessions = AccordoJson.decodeFromString<List<SessionRow>>(api.sessions())
    }

    fun endDeviceSession(id: String) = carefully {
        val api = client ?: return@carefully
        sessionBusy = id
        try {
            api.endSession(id)
            deviceSessions = AccordoJson.decodeFromString<List<SessionRow>>(api.sessions())
        } finally {
            sessionBusy = null
        }
    }

    /** Signs out every other device. The server hands this one a new token: it is saved, and the
     *  live connection restarts with it. */
    fun endOtherDeviceSessions() = carefully {
        val api = client ?: return@carefully
        val user = me ?: return@carefully
        sessionBusy = "others"
        try {
            val token = api.endOtherSessions()
            saveSession(accountKey(user.id, user.username, api.base()), api.base(), token)
            subscribe(user)
            deviceSessions = AccordoJson.decodeFromString<List<SessionRow>>(api.sessions())
            settingsNotice = "Every other device is signed out"
        } finally {
            sessionBusy = null
        }
    }

    /** The network everything on-chain uses from now on (kept on this phone). */
    fun selectChain(id: String) {
        chainId = id
        Prefs.chainId = id
    }

    /** The server's networks. The pick stays if the server still lists it; else Base, else the
     *  first — the desktop's rule. A failure leaves the picker empty, not the sign-in broken. */
    private suspend fun loadChains() {
        val api = client ?: return
        runCatching {
            val list = AccordoJson.decodeFromString<List<Chain>>(api.chains())
            chains = list
            if (list.none { it.id == chainId }) {
                val fallback = list.firstOrNull { it.id == api.defaultChainId() } ?: list.firstOrNull()
                fallback?.let { selectChain(it.id) }
            }
        }
    }

    /** The server this session talks to (the profile screen shows it). */
    fun serverUrl(): String = server

    /** A server path as a full URL, for the image loader. */
    fun url(path: String?): String? {
        if (path.isNullOrBlank()) return null
        return if (path.startsWith("http")) path else absoluteUrl(server, path)
    }

    /** The chat key's fingerprint: what another device compares to be sure it's us. */
    fun keyFingerprint(): String? = me?.e2eePublicKey?.takeIf { it.isNotBlank() }?.let { fingerprintOf(it) }

    private suspend fun refreshFeed() {
        val open = room ?: return
        loadMessages(open)
    }


    fun logout() {
        viewModelScope.launch { runCatching { forgetSession() } }
        subscription?.stop()
        subscription = null
        client = null
        keyring = null
        me = null
        rooms = emptyList()
        rawRooms.clear()
        messages = emptyList()
        roomKeys = emptyList()
        keyNotice = null
        room = null
        settingsSection = null
        deviceSessions = null
        chains = emptyList()
        stage = Stage.Login
    }

    /**
     * This device's chat key: the stored one, or a fresh pair saved at once. The keyring needs it to
     * open the envelopes that carry room keys.
     */
    private fun connectChatKey(api: Client, user: User): RoomKeyring {
        val base = api.base()
        val account = accountKey(user.id, user.username, base)
        val ring = RoomKeyring(user.id, account)
        val stored = loadChatKey(account, previousAccountKey(user.id, base), user.id, user.e2eePublicKey)
        val device = stored ?: DeviceKey.generate().also { saveChatKey(account, it) }
        ring.useDeviceKey(device)
        return ring
    }

    private suspend fun loadRooms() {
        val api = client ?: return
        val array = AccordoJson.parseToJsonElement(api.rooms()).jsonArray
        rooms = array.map { AccordoJson.decodeFromJsonElement(Room.serializer(), it) }
        rawRooms.clear()
        for (element in array) {
            val key = element.jsonObject["room_key"]?.jsonPrimitive?.contentOrNull ?: continue
            rawRooms[key] = element.toString()
        }
    }

    /**
     * The open room's keys. A sealed room whose envelope isn't here (or whose sender's key changed)
     * isn't an error for the screen: the feed still opens, with a line on why it stays sealed.
     */
    private fun loadRoomKeys(room: Room) {
        keyNotice = null
        roomKeys = emptyList()
        val ring = keyring ?: return
        val raw = rawRooms[room.roomKey] ?: return
        try {
            roomKeys = ring.open(raw)
        } catch (e: MobileException.Keys) {
            keyNotice = e.detail
        }
    }

    private suspend fun loadMessages(room: Room) {
        val api = client ?: return
        messages = AccordoJson.decodeFromString<List<Message>>(
            api.messages(room.roomKey, beforeId = null, limit = 50L)
        )
    }

    /** Live updates: the core pushes events from its own thread; refresh what is open. */
    private fun subscribe(user: User) {
        val api = client ?: return
        subscription?.stop()
        subscription = api.subscribe(user.id, user.username, object : SocketListener {
            override fun onEvent(event: String) {
                if (event.contains("\"kind\":\"event\"")) {
                    viewModelScope.launch {
                        runCatching {
                            val open = room
                            if (open != null) {
                                loadRoomKeys(open)
                                loadMessages(open)
                            } else {
                                loadRooms()
                            }
                        }
                    }
                }
            }
        })
    }

    /** Runs a block with the busy flag and turns core errors into the line above the buttons. */
    private fun carefully(block: suspend () -> Unit) {
        viewModelScope.launch {
            busy = true
            error = null
            try {
                block()
            } catch (e: MobileException) {
                error = readable(e)
            } catch (e: Exception) {
                error = e.message ?: e.toString()
            } finally {
                busy = false
            }
        }
    }

    /** The core's own sentence, not the debug form (`status=401, detail=…`). */
    private fun readable(e: MobileException): String = when (e) {
        is MobileException.Api -> e.detail
        is MobileException.Json -> e.detail
        is MobileException.Store -> e.detail
        is MobileException.Wallet -> e.detail
        is MobileException.Keys -> e.detail
        is MobileException.SealFailed -> "The message could not be sealed for this room"
    }

    private companion object {
        const val TTL_MINUTES = 30
        const val DEFAULT_SERVER = "https://accordo.me"
        const val LOCKED = "🔒 sealed message"
    }
}
