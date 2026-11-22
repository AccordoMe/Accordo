// SPDX-License-Identifier: GPL-3.0-or-later
//! Prints vaults sealed with a password, for tests and for the mock server: the app can only unlock
//! something a real vault produced, and `encrypt_vault` is exactly what the web app writes.
//!
//!   cargo run --example make_vault -- "pw" "0xdeadbeef"
//!
//! → {"check":{"salt":…},"wallet":{"salt":…}}

use accordo_core::vault::{encrypt_vault, Vault};
use accordo_core::wallet_session::CHECK;

fn main() {
    let mut args = std::env::args().skip(1);
    let password = args.next().unwrap_or_else(|| "pw".into());
    let secret = args.next().unwrap_or_else(|| "0xdeadbeefcafebabe".into());

    let check: Vault = encrypt_vault(CHECK, &password);
    let wallet: Vault = encrypt_vault(&secret, &password);
    println!(
        "{}",
        serde_json::json!({ "check": check, "wallet": wallet, "secret": secret })
    );
}
