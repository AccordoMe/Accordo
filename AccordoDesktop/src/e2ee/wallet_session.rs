// SPDX-License-Identifier: GPL-3.0-or-later
//! The wallet unlock session (derived keys held in memory) — now in the shared core, so the phone works the same way. Re-exported here so every
//! `crate::e2ee::wallet_session::…` path keeps working.
//!
//! Plus the app's side of an unlock kept between launches (lock timer on): the secret store is
//! read and written off the UI thread, like the chat key.

pub use accordo_core::wallet_session::*;

use std::sync::Arc;

use zeroize::Zeroizing;

use crate::e2ee::keystore::blocking;

/// The account's unlock kept from an earlier launch (None: nothing kept, or it ran out).
pub async fn restore(account: String) -> Option<Arc<Restored>> {
    blocking(move || Ok(load_persisted(&account, chrono::Utc::now().timestamp_millis()))).await.ok().flatten().map(Arc::new)
}

/// Keeps the unlock (`WalletSession::persisted`) for the account until its deadline.
pub async fn keep(account: String, json: Zeroizing<String>) -> Result<(), String> {
    blocking(move || save_persisted(&account, &json)).await
}

/// Removes what these accounts kept (the user locked, logged out, or deleted it in Storage).
pub async fn forget(accounts: Vec<String>) {
    let _ = blocking(move || {
        accounts.iter().for_each(|a| forget_persisted(a));
        Ok(())
    })
    .await;
}
