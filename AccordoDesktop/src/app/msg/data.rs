// SPDX-License-Identifier: GPL-3.0-or-later
//! Data arriving: the messages of this area, handled by the app.
//!
//! The answers to requests the app makes itself: networks and tokens, the account's wallets, `/me`,
//! a token's logo, an avatar picture, the store being written.
//!
//! Kept apart from the handler that answers them so the types can be read in one place;
//! the handler is `app/loads.rs`.

use super::super::*;

#[derive(Debug, Clone)]
pub enum DataMsg {
    /// An avatar picture loaded (or didn't) — crate::avatars.
    AvatarLoaded(u64, Option<crate::avatars::Pic>),
    /// Something waits for an avatar: load it.
    AvatarPump,
    /// A token's logo, by the token's (lowercase) address.
    TokenLogo(String, Option<iced::widget::image::Handle>),
    /// The networks this server knows, with their contracts.
    ChainsLoaded(Result<Vec<Chain>, ApiError>),
    /// One chain's tokens.
    TokensLoaded(Result<(String, Vec<Token>), ApiError>),
    /// The account's wallets as the server has them now (after a change on another device).
    WalletsSynced(Result<crate::api::auth::User, ApiError>),
    /// A wallet shown in / hidden from the profile (true: shown) — saved, or not: then the list
    /// it had before comes back.
    PublicWalletsSaved(bool, Vec<String>, Result<crate::api::auth::User, ApiError>),
    /// `/me` again: the session's user, refreshed.
    UserRefreshed(Result<crate::api::auth::User, ApiError>),
    /// state.json was written (or couldn't be).
    StoreSaved(Result<(), String>),
    /// "Not now" on the unsaved-changes banner.
    UnsavedRetry,
    /// The banner's backup button.
    UnsavedBackup,
    /// An account's records were kept (its index on disk): (kept, total).
    UnsavedRetried((usize, usize)),
}
