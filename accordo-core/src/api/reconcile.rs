// SPDX-License-Identifier: GPL-3.0-or-later
//! Catching the database up with the chain (src/api/reconcile.ts): a card that sees the contract
//! ahead of the database asks the backend to find the missing events itself (eth_getLogs) and
//! apply them. Once per item per run; guests can't, and failures are quiet (None).

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;

use super::client::Api;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Invoices,
    Cheques,
}

static ASKED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// The backend's answer (`applied`: what it caught up), or None when skipped / failed.
pub async fn reconcile_once(api: Api, kind: Kind, id: String, signed_in: bool) -> Option<Value> {
    let kind = match kind {
        Kind::Invoices => "invoices",
        Kind::Cheques => "cheques",
    };
    let key = format!("{kind}:{}", id.to_lowercase());
    if !signed_in || !ASKED.lock().ok()?.insert(key) {
        return None;
    }
    api.post_empty(&format!("/api/{kind}/{id}/reconcile")).await.ok()
}

/// Whether the answer says something was applied (then the lists should reload).
pub fn applied_any(answer: &Value) -> bool {
    answer.get("applied").and_then(Value::as_array).is_some_and(|a| !a.is_empty())
}
