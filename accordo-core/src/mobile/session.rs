// SPDX-License-Identifier: GPL-3.0-or-later
//! Saved sign-ins for the phone: the token goes where the desktop keeps it (the host's secret store —
//! the Android Keystore here), plus one pointer to the account that was last open, so a restart
//! lands in the chat list instead of the sign-in form.
//!
//! The chat key is stored separately (`keystore`), under the same account key, and survives the same
//! way. Nothing secret is written to a file.

use crate::api::session_store::{self, Saved};
use crate::mobile::MobileError;
use crate::secrets;

/// Which account the app was last open as (`1:alina@https://accordo.me`).
const ACTIVE: &str = "session:active";

/// Remembers a signed-in session: `{"account","base","token"}` is what `load_session` gives back.
#[uniffi::export(async_runtime = "tokio")]
pub async fn save_session(account: String, base: String, token: String) -> Result<(), MobileError> {
    session_store::save(account.clone(), Saved { base, token, moved_from: None })
        .await
        .map_err(|message| MobileError::Store { detail: message })?;
    secrets::set(ACTIVE, &account)?;
    Ok(())
}

/// The last session as JSON, or null when there isn't one (first launch, signed out, wiped keystore).
#[uniffi::export(async_runtime = "tokio")]
pub async fn load_session() -> Result<Option<String>, MobileError> {
    let Some(account) = secrets::get(ACTIVE) else { return Ok(None) };
    let Some(saved) = session_store::load_account(account.clone(), None).await else { return Ok(None) };
    Ok(Some(serde_json::json!({ "account": account, "base": saved.base, "token": saved.token }).to_string()))
}

/// Signs out on this device: the token, the account pointer and the old single-entry name.
#[uniffi::export(async_runtime = "tokio")]
pub async fn forget_session() {
    match secrets::get(ACTIVE) {
        Some(account) => session_store::clear(Some(account)).await,
        None => session_store::clear(None).await,
    }
    secrets::remove(ACTIVE);
}

/// The host's own directory for this app's files (pins, settings): Android has no XDG or HOME.
#[uniffi::export]
pub fn set_config_dir(path: String) {
    crate::api::client::set_config_dir(std::path::PathBuf::from(path));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::SecretStore;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Memory(Mutex<HashMap<String, String>>);

    impl SecretStore for Memory {
        fn get(&self, key: &str) -> Option<String> {
            self.0.lock().ok()?.get(key).cloned()
        }
        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            self.0.lock().map_err(|e| e.to_string())?.insert(key.into(), value.into());
            Ok(())
        }
        fn remove(&self, key: &str) {
            if let Ok(mut map) = self.0.lock() {
                map.remove(key);
            }
        }
    }

    #[test]
    fn a_session_survives_and_is_forgotten() {
        // Blocking runtime inside a sync test: the process-wide store is shared, so the guard must
        // not live across an await point.
        let _guard = crate::secrets::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
        runtime.block_on(async {
        secrets::install(Arc::new(Memory::default()));
        let account = crate::api::session_store::account_key(1, "alina", "https://accordo.me");

        assert!(load_session().await.expect("empty").is_none());
        save_session(account.clone(), "https://accordo.me".into(), "t-1".into()).await.expect("save");

        let json = load_session().await.expect("load").expect("a session");
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["account"], account.as_str());
        assert_eq!(value["token"], "t-1");
        assert_eq!(value["base"], "https://accordo.me");

        forget_session().await;
        assert!(load_session().await.expect("empty").is_none());
        });
    }

    #[test]
    fn the_host_directory_wins() {
        set_config_dir("/tmp/accordo-mobile-test".into());
        assert_eq!(
            crate::api::client::config_dir().map(|p| p.display().to_string()).as_deref(),
            Some("/tmp/accordo-mobile-test")
        );
    }
}
