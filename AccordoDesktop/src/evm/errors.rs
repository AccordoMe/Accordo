// SPDX-License-Identifier: GPL-3.0-or-later
//! Why a transaction failed, in a sentence a person can act on (txFlow.ts explainTxError,
//! crowdfundContract.ts explainCrowdfundError, nftMinter's custom errors).

use std::fmt;

use alloy_primitives::{keccak256, Bytes, B256};

#[derive(Debug, Clone)]
pub enum TxError {
    /// The network has no http(s) RPC endpoint.
    NoRpc,
    /// The node can't be reached / timed out.
    Network(String),
    /// The node answered with an error (a revert carries its data).
    Rpc { code: i64, message: String, data: Option<Bytes> },
    /// Mined, but reverted.
    Reverted(B256),
    /// Sent, not mined within the wait — it may still go through.
    Pending(B256),
    /// The endpoint serves another chain than the one picked.
    WrongChain { want: u64, got: u64 },
    BadKey,
    BadAddress(String),
    BadAmount(String),
    /// A view returned nothing (no contract there, or an unknown id).
    Empty,
    Decode,
    /// Anything else, already in words.
    Other(String),
    /// A dry run (`Wallet::planner`) reached the real transaction and stopped there, unsent.
    Planned,
}

impl TxError {
    /// The hash of a transaction that went out, if this error happened after sending.
    pub fn hash(&self) -> Option<B256> {
        match self {
            TxError::Reverted(h) | TxError::Pending(h) => Some(*h),
            _ => None,
        }
    }
}

/// Selector of a custom error, from its signature.
fn selector(sig: &str) -> [u8; 4] {
    let h = keccak256(sig.as_bytes());
    [h[0], h[1], h[2], h[3]]
}

/// Custom errors of OpenZeppelin ERC-20 and the Accordo contracts → what to tell the user.
const CUSTOM_ERRORS: &[(&str, &str)] = &[
    ("ERC20InsufficientBalance(address,uint256,uint256)", "Not enough tokens in this wallet for that amount."),
    ("ERC20InsufficientAllowance(address,uint256,uint256)", "Token approval is missing or too low — approve the amount and retry."),
    ("SafeERC20FailedOperation(address)", "The token refused the transfer — check your balance and approval."),
    ("BadMsgValue()", "The amount of native coin sent doesn't match — the price may have just changed. Try again."),
    ("WrongPayment()", "The amount of native coin sent doesn't match — the price may have just changed. Try again."),
    ("ChequeExpired()", "This has expired."),
    ("Expired()", "This has expired."),
    ("ChequeNotFound()", "Not found on-chain."),
    ("UnknownCheque()", "Not found on-chain."),
    ("ChequeAlreadyExists()", "It already exists on-chain — try again."),
    ("ChequeExists()", "It already exists on-chain — try again."),
    ("ContractPaused()", "The contract is paused right now. Try again later."),
    ("InvalidExpiry()", "The end date doesn't fit — it must be in the future and after the last payout."),
    ("InvalidFeeBps()", "The platform fee must be between 0.1% and 5%."),
    ("InvalidFee()", "The platform fee must be between 0.1% and 5%."),
    ("InvalidProof()", "This wallet isn't on the list, or its terms don't match."),
    ("InvalidSchedule()", "The payout schedule doesn't fit the drop."),
    ("NotExpiredYet()", "It hasn't expired yet — you can take the rest back after the end date."),
    ("NotExpired()", "It hasn't expired yet — you can take the rest back after the end date."),
    ("NothingToRefund()", "Nothing left to take back."),
    ("NothingUnlocked()", "Nothing has unlocked yet — wait for the next payout."),
    ("AlreadyClaimed()", "Already claimed."),
    ("OverClaim()", "Already claimed in full."),
    ("AlreadyRefunded()", "Already taken back."),
    ("PermissionDenied()", "This wallet isn't allowed to do that."),
    ("NotSender()", "Only the one who created it can do this."),
    ("NotInRecipientsList()", "This wallet isn't one of the recipients."),
    ("RecipientsRequired()", "Add at least one wallet."),
    ("MaxRecipientsExceeded()", "Too many recipients."),
    ("DuplicateRecipient()", "The same wallet is listed twice."),
    ("TransferFailed()", "The transfer failed."),
    ("WrongSecret()", "The claim code doesn't match."),
    ("NoSlotLeft()", "Every slot of this cheque is already claimed."),
    ("BadSignature()", "The claim signature isn't valid for this wallet."),
    ("SignatureExpired()", "The claim signature has expired."),
    ("SlotFeeTooHigh()", "The creation fee is above the contract's cap."),
    ("InvalidParams()", "The contract rejected these parameters."),
    ("CampaignExists()", "A campaign with this id already exists."),
    ("UnknownCampaign()", "Campaign not found on-chain."),
    ("NotCreator()", "Only the campaign creator can do this."),
    ("GoalNotReached()", "The campaign can only be ended early once its target is reached."),
];

/// AccordoCrowdfund.State names for WrongState(uint8).
const CAMPAIGN_STATES: [&str; 5] = ["unknown", "active", "waiting for the creator's decision", "already claimed", "refunding"];

/// A revert's data → words, if it's one of ours.
pub fn decode_revert(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }
    let sel = [data[0], data[1], data[2], data[3]];
    if sel == selector("WrongState(uint8)") && data.len() >= 36 {
        let state = data[35] as usize;
        return Some(format!("Campaign is {}.", CAMPAIGN_STATES.get(state).copied().unwrap_or("in another state")));
    }
    // Error(string): a require message.
    if sel == selector("Error(string)") {
        use alloy_sol_types::SolValue;
        if let Ok(msg) = String::abi_decode(&data[4..]) {
            // OpenZeppelin 4's "ERC20: transfer amount exceeds balance" and the like, in words.
            return Some(explain_message(&msg).map(str::to_string).unwrap_or_else(|| format!("The contract rejected it: {msg}")));
        }
    }
    CUSTOM_ERRORS.iter().find(|(sig, _)| selector(sig) == sel).map(|(_, text)| text.to_string())
}

fn explain_message(m: &str) -> Option<&'static str> {
    let l = m.to_lowercase();
    let has = |s: &str| l.contains(s);
    if has("insufficient funds") {
        return Some("Not enough native coin to cover the amount plus network gas.");
    }
    if has("exceeds balance") || has("insufficient balance") {
        return Some("Not enough tokens in this wallet for that amount.");
    }
    if has("insufficient allowance") || has("allowance too low") {
        return Some("Token approval is missing or too low — approve the amount and retry.");
    }
    if has("nonce too low") || has("replacement transaction underpriced") || has("underpriced") || has("already known") {
        return Some("Another transaction from this wallet is still pending. Wait for it to confirm, then retry.");
    }
    if has("gas required exceeds") || has("out of gas") || has("intrinsic gas") {
        return Some("The network rejected the gas estimate — the transaction would likely fail.");
    }
    None
}

impl fmt::Display for TxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxError::NoRpc => f.write_str("This network has no RPC endpoint — add one in the network settings (the gear by the network picker)."),
            TxError::Network(_) => f.write_str("The network node is not responding. Try again in a moment."),
            TxError::Rpc { message, data, .. } => {
                if let Some(text) = data.as_ref().and_then(|d| decode_revert(d)) {
                    return f.write_str(&text);
                }
                if let Some(text) = explain_message(message) {
                    return f.write_str(text);
                }
                let l = message.to_lowercase();
                if let Some(reason) = l.find("execution reverted:").map(|i| message[i + "execution reverted:".len()..].trim()).filter(|r| !r.is_empty()) {
                    return write!(f, "The contract rejected it: {}", reason.chars().take(160).collect::<String>());
                }
                if l.contains("revert") {
                    return f.write_str("The contract rejected this transaction.");
                }
                let line = message.lines().next().unwrap_or("Transaction failed").trim();
                f.write_str(&line.chars().take(180).collect::<String>())
            }
            TxError::Reverted(_) => f.write_str("The transaction was reverted on-chain — nothing was sent. Check the amount and try again."),
            TxError::Pending(h) => write!(f, "The transaction was sent but isn't confirmed yet ({h:#x}). Check it in the explorer before retrying."),
            TxError::WrongChain { want, got } => write!(f, "The RPC endpoint serves chain {got}, not {want} — check the network settings."),
            TxError::BadKey => f.write_str("This wallet's key can't sign — re-import it."),
            TxError::BadAddress(a) => write!(f, "Not a wallet address: {a}"),
            TxError::BadAmount(a) => write!(f, "Not a valid amount: {a}"),
            TxError::Empty => f.write_str("Not found on-chain."),
            TxError::Decode => f.write_str("The node gave an answer this app can't read."),
            TxError::Other(s) => f.write_str(s),
            TxError::Planned => f.write_str("Simulated only — nothing was sent."),
        }
    }
}

impl std::error::Error for TxError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_selectors() {
        // The selectors txFlow.ts lists by hex.
        assert_eq!(selector("ERC20InsufficientBalance(address,uint256,uint256)"), [0xe4, 0x50, 0xd3, 0x8c]);
        assert_eq!(selector("ChequeExpired()"), [0x63, 0x9a, 0x80, 0x2a]);
        assert_eq!(selector("AlreadyClaimed()"), [0x64, 0x6c, 0xf5, 0x58]);
        assert_eq!(decode_revert(&[0x64, 0x6c, 0xf5, 0x58]).as_deref(), Some("Already claimed."));
        let mut wrong_state = selector("WrongState(uint8)").to_vec();
        wrong_state.extend([0u8; 31]);
        wrong_state.push(2);
        assert_eq!(decode_revert(&wrong_state).as_deref(), Some("Campaign is waiting for the creator's decision."));
    }

    #[test]
    fn messages() {
        let e = TxError::Rpc { code: -32000, message: "insufficient funds for gas * price + value".into(), data: None };
        assert_eq!(e.to_string(), "Not enough native coin to cover the amount plus network gas.");
        let e = TxError::Rpc { code: 3, message: "execution reverted: Ownable: caller is not the owner".into(), data: None };
        assert_eq!(e.to_string(), "The contract rejected it: Ownable: caller is not the owner");
    }
}
