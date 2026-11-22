// SPDX-License-Identifier: GPL-3.0-or-later
//! Reference data (src/api/data.ts, Web3Context.tsx): the networks Accordo works on.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::client::{Api, ApiResult};

/// The web app's default network: Base.
pub const DEFAULT_CHAIN_ID: &str = "8453";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChainRaw {
    name: String,
    /// A number for EVM chains; the web app also knows string ids (e.g. "Tron").
    chain_id_dec: serde_json::Value,
    #[serde(default)]
    testnet: bool,
    native_currency: Option<NativeCurrency>,
    #[serde(default)]
    block_explorer_urls: Vec<String>,
    explorer_url: Option<String>,
    #[serde(default)]
    rpc_urls: Vec<String>,
    /// The developer's own chain (Anvil / the test validator): gets a faucet.
    #[serde(default)]
    local: bool,
    /// "solana" for Solana networks (isSolanaNetwork), else EVM.
    #[serde(default)]
    vm: Option<String>,
    /// Accordo's contracts here: name → address (and some `*_metadata` objects, skipped).
    #[serde(default)]
    contracts: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct NativeCurrency {
    symbol: String,
}

/// One entry of the network selector. Serialize: the phone gets it as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Chain {
    pub id: String,
    pub name: String,
    pub testnet: bool,
    /// "ETH", "BNB", "POL"…
    pub native_symbol: String,
    /// Where transactions are looked at (no trailing slash) — the server's; the user's own
    /// choice is in the store's chain overrides (App::explorer_of).
    pub explorer: Option<String>,
    /// The server's RPC endpoints (the user's own replace them: App::rpc_urls).
    pub rpc_urls: Vec<String>,
    pub local: bool,
    pub solana: bool,
    /// Accordo's contracts on it: "AccordoCheques" → address.
    pub contracts: std::collections::HashMap<String, String>,
}

impl Chain {
    /// The address of one of Accordo's contracts here ("AccordoCheques", "AccordoInvoice"…).
    pub fn contract(&self, name: &str) -> Option<&str> {
        self.contracts.get(name).map(String::as_str)
    }

    /// The numeric chain id of an EVM network.
    pub fn evm_id(&self) -> Option<u64> {
        self.id.parse().ok()
    }
}

/// Shown like the web selector: "Base Sepolia (Test)".
impl fmt::Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.testnet {
            write!(f, "{} (Test)", self.name)
        } else {
            f.write_str(&self.name)
        }
    }
}

pub async fn chains(api: Api) -> ApiResult<Vec<Chain>> {
    let raw: Vec<serde_json::Value> = api.get("/api/data/chains").await?;
    Ok(parse_chains(raw))
}

/// The networks the server listed; an entry that isn't a network (no name, no id — e.g. something
/// pasted into chain.data.json by mistake) is skipped and logged, so one bad entry doesn't take
/// every network away.
fn parse_chains(raw: Vec<serde_json::Value>) -> Vec<Chain> {
    raw.into_iter()
        .enumerate()
        .filter_map(|(i, v)| match serde_json::from_value::<ChainRaw>(v) {
            Ok(c) if !c.chain_id_dec.is_null() && !c.name.trim().is_empty() => Some(c),
            Ok(_) => {
                tracing::warn!("network #{i} from the server has no id or name; skipped");
                None
            }
            Err(e) => {
                tracing::warn!("network #{i} from the server isn't a network ({e}); skipped");
                None
            }
        })
        .map(|c| Chain {
            id: match c.chain_id_dec {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            },
            name: c.name,
            testnet: c.testnet,
            native_symbol: c.native_currency.map(|n| n.symbol).unwrap_or_else(|| "ETH".into()),
            explorer: c.block_explorer_urls.into_iter().next().or(c.explorer_url).map(|u| u.trim_end_matches('/').to_string()),
            rpc_urls: c.rpc_urls,
            local: c.local,
            solana: c.vm.as_deref().is_some_and(|v| v.eq_ignore_ascii_case("solana")),
            contracts: contract_keys(c.contracts),
        })
        .collect()
}

/// Keys from before the contracts were renamed (AccordoCheque_v5 → AccordoDrop, AccordoCheques_v6
/// → AccordoTask). An updated server already renames them (app/contract_keys.py); this is for one
/// that isn't yet.
const LEGACY_KEYS: [(&str, &str); 4] = [
    ("AccordoChequesV5", "AccordoDrop"),
    ("AccordoCheques_v5", "AccordoDrop"),
    ("AccordoChequesV6", "AccordoTask"),
    ("AccordoCheques_v6", "AccordoTask"),
];

/// The addresses under today's keys (where a network has both, today's wins); the deploy notes
/// (`*_metadata` objects) are skipped.
fn contract_keys(raw: std::collections::HashMap<String, serde_json::Value>) -> std::collections::HashMap<String, String> {
    let mut found: Vec<(String, String, bool)> = raw
        .into_iter()
        .filter_map(|(k, v)| {
            let a = v.as_str().filter(|a| !a.is_empty())?.to_string();
            Some(match LEGACY_KEYS.iter().find(|(old, _)| *old == k) {
                Some((_, new)) => (new.to_string(), a, true),
                None => (k, a, false),
            })
        })
        .collect();
    found.sort_by_key(|(_, _, legacy)| *legacy);
    let mut out = std::collections::HashMap::new();
    for (k, a, _) in found {
        out.entry(k).or_insert(a);
    }
    out
}

#[cfg(test)]
mod chain_tests {
    #[test]
    fn a_stray_entry_doesnt_drop_the_others() {
        let raw = serde_json::json!([
            {"name": "Base", "chainIdDec": 8453, "rpcUrls": ["https://mainnet.base.org"], "contracts": {"AccordoCheques": "0x1", "AccordoCheques_metadata": {"tx": "0x2"}}},
            {"name": null, "chainIdDec": null, "chain": {"name": "Local Anvil"}, "tokens": []},
            {"chainIdDec": 1},
            {"name": "Solana (local)", "chainIdDec": "solana-local", "vm": "solana"},
        ]);
        let chains = super::parse_chains(serde_json::from_value(raw).unwrap());
        let names: Vec<&str> = chains.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Base", "Solana (local)"]);
        assert_eq!(chains[0].id, "8453");
        assert_eq!(chains[0].contract("AccordoCheques"), Some("0x1"));
        assert!(chains[1].solana && chains[1].id == "solana-local");
    }

    #[test]
    fn old_contract_keys_read_as_new() {
        let raw = serde_json::json!([
            {"name": "Arbitrum One", "chainIdDec": 42161, "contracts": {"AccordoCheques": "0xc", "AccordoCheques_v6": "0x6", "AccordoChequesV5": "0x5", "AccordoCheques_v6_metadata": {"tx": "0x2"}}},
            {"name": "Both", "chainIdDec": 1, "contracts": {"AccordoChequesV6": "0xold", "AccordoTask": "0xnew"}},
        ]);
        let chains = super::parse_chains(serde_json::from_value(raw).unwrap());
        assert_eq!((chains[0].contract("AccordoCheques"), chains[0].contract("AccordoTask"), chains[0].contract("AccordoDrop")), (Some("0xc"), Some("0x6"), Some("0x5")));
        assert_eq!(chains[0].contracts.len(), 3);
        assert_eq!(chains[1].contract("AccordoTask"), Some("0xnew"));
    }
}

/// One token of a network (/api/data/tokens, from public/tokens.json).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Token {
    pub symbol: String,
    pub name: String,
    pub address: String,
    pub decimals: u8,
    /// Its picture (tokens.json logoURI).
    #[serde(default, rename = "logoURI")]
    pub logo: Option<String>,
}

/// "USDC — USD Coin": what the token picker shows and searches.
impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} — {}", self.symbol, self.name)
    }
}

impl Token {
    /// The network's own coin, for networks without a token list (TokenModal.tsx does the same).
    pub fn native(chain: &Chain) -> Self {
        Self { symbol: chain.native_symbol.clone(), name: chain.name.clone(), address: format!("0x{}", "0".repeat(40)), decimals: 18, logo: None }
    }
}

/// The token list of one network, like the web app's loadChainTokens(): the server answers
/// `{"<chainId>": [...], "_took_ms": …}`; entries that don't parse are skipped.
pub async fn tokens(api: Api, chain_id: String) -> ApiResult<(String, Vec<Token>)> {
    let path = format!("/api/data/tokens?chainId={}", super::client::enc(&chain_id));
    let body: std::collections::HashMap<String, serde_json::Value> = api.get(&path).await?;
    let list = match body.get(&chain_id) {
        Some(serde_json::Value::Array(items)) => items.iter().filter_map(|t| serde_json::from_value(t.clone()).ok()).collect(),
        _ => Vec::new(),
    };
    Ok((chain_id, list))
}

/// The key /api/data/prices uses: "<chainId>:native" or "<chainId>:<lowercase address>".
pub fn price_key(chain_id: &str, token: &str) -> String {
    let t = token.trim().to_lowercase();
    if t.is_empty() || t == format!("0x{}", "0".repeat(40)) || t == "native" { format!("{chain_id}:native") } else { format!("{chain_id}:{t}") }
}

/// USD prices (DefiLlama via the backend) for up to 50 keys; unpriced tokens are absent.
pub async fn prices(api: Api, keys: Vec<String>) -> ApiResult<std::collections::HashMap<String, f64>> {
    let mut out = std::collections::HashMap::new();
    for batch in keys.chunks(50) {
        let v: serde_json::Value = api.get(&format!("/api/data/prices?tokens={}", super::client::enc(&batch.join(",")))).await?;
        let v = super::client::unwrap_data(v);
        if let Some(map) = v.get("prices").and_then(|p| p.as_object()) {
            for (k, p) in map {
                if let Some(n) = p.as_f64() {
                    out.insert(k.clone(), n);
                }
            }
        }
    }
    Ok(out)
}
