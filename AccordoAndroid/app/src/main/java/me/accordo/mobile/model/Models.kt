// SPDX-License-Identifier: GPL-3.0-or-later
// The JSON shapes the core hands over. They are the server's own fields (the same ones the desktop
// deserializes), so nothing here invents a field: unknown keys are ignored, so a server that grows a
// field never breaks the app.

package me.accordo.mobile.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement

/** The one parser: lenient about new fields, strict about what we ask for. */
val AccordoJson = Json {
    ignoreUnknownKeys = true
    isLenient = true
    explicitNulls = false
}

/** What `Client.login` answers: the token the client keeps, and the user. */
@Serializable
data class LoginAnswer(
    val token: String,
    val user: User,
    @SerialName("recovery_codes") val recoveryCodes: List<String>? = null,
)

/** `api::auth::User`, the fields a phone shows. */
@Serializable
data class User(
    val id: Long,
    val username: String,
    val email: String? = null,
    @SerialName("display_name") val displayName: String? = null,
    val avatar: String? = null,
    @SerialName("e2ee_public_key") val e2eePublicKey: String? = null,
    // Settings (the desktop's cards read the same fields).
    @SerialName("is_admin") val isAdmin: Boolean = false,
    val verified: Boolean? = null,
    @SerialName("totp_enabled") val totpEnabled: Boolean = false,
    @SerialName("recovery_codes_left") val recoveryCodesLeft: Int = 0,
    @SerialName("api_key") val apiKey: String? = null,
    @SerialName("api_login") val apiLogin: Boolean = false,
    @SerialName("has_key_backup") val hasKeyBackup: Boolean = false,
) {
    val name: String get() = displayName?.takeIf { it.isNotBlank() } ?: username
}

/** `api::chat::ChatRoomItem`, the fields the chat list draws. */
@Serializable
data class Room(
    val id: Long,
    @SerialName("room_key") val roomKey: String,
    @SerialName("chat_type") val chatType: String = "public",
    val title: String = "",
    @SerialName("custom_title") val customTitle: String? = null,
    val avatar: String? = null,
    @SerialName("last_message") val lastMessage: String? = null,
    @SerialName("last_timestamp") val lastTimestamp: String? = null,
    @SerialName("is_member") val isMember: Boolean? = null,
    @SerialName("member_count") val memberCount: Long? = null,
) {
    val name: String get() = customTitle?.takeIf { it.isNotBlank() } ?: title.ifBlank { roomKey }
}

/** `api::chat::ThreadSummary`: how many replies hang under a message. */
@Serializable
data class ThreadSummary(val count: Long = 0)

/** `api::chat::ChatMessageItem`, the fields the feed draws. */
@Serializable
data class Message(
    val id: Long,
    @SerialName("room_id") val roomId: String = "",
    @SerialName("sender_id") val senderId: Long? = null,
    @SerialName("sender_username") val senderUsername: String = "",
    @SerialName("sender_display_name") val senderDisplayName: String? = null,
    @SerialName("sender_avatar") val senderAvatar: String? = null,
    val content: String = "",
    @SerialName("message_type") val messageType: String = "text",
    @SerialName("created_at") val createdAt: String = "",
    /** emoji → the ids of whoever reacted. */
    val reactions: Map<String, List<Long>>? = null,
    @SerialName("is_pinned") val isPinned: Boolean? = null,
    @SerialName("thread_root_id") val threadRootId: Long? = null,
    @SerialName("thread_summary") val threadSummary: ThreadSummary? = null,
) {
    val senderName: String get() = senderDisplayName?.takeIf { it.isNotBlank() } ?: senderUsername

    /** "2026-11-22T09:14:03…" → "09:14" — the desktop shows the same clock time. */
    val clock: String get() = createdAt.substringAfter('T').take(5)
}

/** `api::auth::RegistrationStatus`: what the sign-up form should offer. */
@Serializable
data class SignupStatus(
    val enabled: Boolean = false,
    @SerialName("max_users") val maxUsers: Long = 0,
    @SerialName("current_users") val currentUsers: Long = 0,
    @SerialName("remaining_spots") val remainingSpots: Long? = null,
    @SerialName("invite_code_required") val inviteCodeRequired: Boolean = false,
) {
    /** Nobody may sign up: the form gives way to the waitlist. */
    val closed: Boolean get() = !enabled

    /** Open, but the account needs an access code. */
    val inviteOnly: Boolean get() = inviteCodeRequired || remainingSpots == 0L
}

/** The `{ "status": "…" }` answer the auth endpoints give (waitlist, email sent, password changed). */
@Serializable
data class Answer(val status: String = "", val message: String? = null)

/** What `loadSession()` hands back: whose token it is, where, and the token itself. */
@Serializable
data class SavedSession(val account: String, val base: String, val token: String)

/** `accordo_core::vault::SecureWallet`: the address openly, the key sealed under the master password. */
@Serializable
data class Wallet(
    val id: String = "",
    val name: String = "",
    val address: String = "",
    val chain: String? = null,
    /** Kept as-is: the core opens it, the app never looks inside. */
    val vault: JsonElement? = null,
) {
    val short: String get() = if (address.length > 14) "${address.take(8)}…${address.takeLast(6)}" else address
}

/** One network of the picker (`Client.chains`): the desktop's `api::data::Chain`. */
@Serializable
data class Chain(
    val id: String,
    val name: String,
    val testnet: Boolean = false,
    @SerialName("native_symbol") val nativeSymbol: String = "",
    val local: Boolean = false,
    val solana: Boolean = false,
)

/** Where the account is signed in (`Client.sessions`), with the core's wording for the dates. */
@Serializable
data class SessionRow(
    val session: DeviceSession,
    @SerialName("signed_in") val signedIn: String? = null,
    val active: String? = null,
)

@Serializable
data class DeviceSession(
    val id: String,
    val device: String,
    @SerialName("via_api_key") val viaApiKey: Boolean = false,
    val current: Boolean = false,
)
