// SPDX-License-Identifier: GPL-3.0-or-later
//! The parts of the Accordo contracts the app calls — the same signatures as the web's ABIs
//! (chequeContract.ts, taskCheques.ts, drops.ts, invoiceContract.ts, crowdfundContract.ts). A test
//! checks every selector here against the compiled ABIs in contracts/out.

#![allow(clippy::too_many_arguments)]

use alloy_sol_types::sol;

sol! {
    interface IERC20 {
        function approve(address spender, uint256 amount) returns (bool);
        function allowance(address owner, address spender) view returns (uint256);
        function balanceOf(address account) view returns (uint256);
        function decimals() view returns (uint8);
        function symbol() view returns (string);
        function name() view returns (string);
    }

    /// Multicall3, at the same address on practically every EVM chain.
    interface Multicall3 {
        struct Call3 {
            address target;
            bool allowFailure;
            bytes callData;
        }
        struct Result {
            bool success;
            bytes returnData;
        }
        function aggregate3(Call3[] calls) payable returns (Result[] returnData);
    }

    /// AccordoCheques: cheques (claimed by revealing the secret) and swaps.
    interface AccordoCheques {
        struct ChequeInfo {
            address from;
            address token;
            uint256 amountPerRecipient;
            address[] to;
            bool[] claimed;
            uint64 createdAt;
            uint64 expiresAt;
            uint16 feeBps;
            bool refunded;
        }
        struct SwapInfo {
            address spender;
            address receiver;
            address tokenIn;
            uint256 amountIn;
            address tokenOut;
            uint256 amountOut;
            bool claimed;
            uint64 createdAt;
            uint64 expiresAt;
            uint16 feeBps;
            bool refunded;
        }
        function InitCheque(bytes32 chequeId, address tokenAddr, uint256 totalAmount, address[] _to, uint64 expiresAt, uint16 feeBps) payable returns (bytes32);
        function InitSwapCheque(bytes32 chequeId, address _receiver, address _tokenIn, uint256 _amountIn, address _tokenOut, uint256 _amountOut, uint64 expiresAt, uint16 feeBps) payable returns (bytes32);
        function CashOutCheque(bytes32 secret);
        function CashOutSwapCheque(bytes32 secret) payable;
        function RefundCheque(bytes32 chequeId);
        function RefundSwapCheque(bytes32 chequeId);
        function ExtendCheque(bytes32 chequeId, uint64 newExpiresAt) returns (uint64);
        function ExtendSwapCheque(bytes32 chequeId, uint64 newExpiresAt) returns (uint64);
        function getChequeInfo(bytes32 id) view returns (ChequeInfo info);
        function getSwapDetail(bytes32 id) view returns (SwapInfo info);
        function getFee() view returns (uint256 minBps, uint256 defaultBps, uint256 maxBps, uint256 denominator);
        function treasury() view returns (address);
    }

    /// AccordoTask: claims signed with a one-time claim key (EIP-712), relayable.
    interface AccordoTask {
        struct Cheque {
            address sender;
            address token;
            address claimSigner;
            uint128 amountPerSlot;
            uint64 createdAt;
            uint64 expiresAt;
            uint16 feeBps;
            uint16 openSlots;
            uint16 openClaimed;
            uint16 namedCount;
            uint16 claimedCount;
            bool refunded;
        }
        function createCheque(bytes32 salt, address token, uint256 total, address[] named, uint16 openSlots, address claimSigner, uint64 expiresAt, uint16 feeBps) payable returns (bytes32);
        function claim(bytes32 id, address recipient, uint256 relayerFee, uint64 deadline, bytes signature);
        function refund(bytes32 id);
        function extend(bytes32 id, uint64 newExpiresAt);
        function isClaimed(bytes32 id, address recipient) view returns (bool);
        function getCheque(bytes32 id) view returns (Cheque cheque, address[] named, bool[] namedClaimed);
    }

    /// The EIP-712 message an AccordoTask claim key signs.
    struct Claim {
        bytes32 chequeId;
        address recipient;
        uint256 relayerFee;
        uint64 deadline;
    }

    /// AccordoDrop: Merkle drops.
    interface AccordoDrop {
        struct Schedule {
            uint64 start;
            uint32 interval;
            uint16 periods;
            uint32 cliff;
            uint16 upfrontBps;
        }
        struct MerkleChequeInfo {
            address from;
            address token;
            uint256 totalAmount;
            uint256 totalClaimedAmount;
            bytes32 merkleRoot;
            uint32 totalRecipientsCount;
            uint32 claimedRecipientsCount;
            uint64 createdAt;
            uint64 expiresAt;
            uint16 feeBps;
            bool refunded;
            uint64 vestingEnd;
        }
        function InitMerkleCheque(bytes32 salt, address tokenAddr, uint256 totalAmount, bytes32 merkleRoot, uint32 totalRecipientsCount, uint64 expiresAt, uint16 feeBps, uint64 vestingEnd) payable returns (bytes32);
        function CashOutMerkleCheque(bytes32 chequeId, uint32 slot, uint256 amount, Schedule schedule, bytes32[] merkleProof);
        function RefundMerkleCheque(bytes32 chequeId);
        function ExtendMerkleCheque(bytes32 chequeId, uint64 newExpiresAt);
        function getMerkleChequeInfo(bytes32 chequeId) view returns (MerkleChequeInfo info);
        function slotClaimedAmount(bytes32 chequeId, uint32 slot) view returns (uint256);
        function creationFee(uint32 recipients) view returns (uint256);
        function slotFee() view returns (uint256);
        function maxDropFee() view returns (uint256);
    }

    /// InvoiceHub (AccordoInvoice.sol).
    interface InvoiceHub {
        struct Invoice {
            address merchant;
            address token;
            uint128 amount;
            bool fact_paid;
            uint128 paid;
            uint64 createdAt;
            uint64 dueAt;
            uint16 feeBps;
            address payer;
            bytes32 codeHash;
            bool canceled;
            uint256 count;
        }
        struct NewInvoice {
            bytes32 id;
            address merchant;
            address token;
            uint128 amount;
            uint64 dueAt;
            uint16 feeBps;
            address payer;
            bytes32 codeHash;
        }
        function createInvoice(bytes32 id, address merchant, address token, uint128 amount, uint64 dueAt, uint16 feeBps, address payer, bytes32 codeHash);
        function createInvoices(NewInvoice[] items);
        function payETH(bytes32 id, uint256 amount, string secretCode) payable;
        function payERC20(bytes32 id, uint256 amount, string secretCode);
        function cancelInvoice(bytes32 id);
        function getInvoice(bytes32 id) view returns (Invoice inv);
        function DEFAULT_FEE_BPS() view returns (uint256);
        function FEE_DENOMINATOR() view returns (uint256);
    }

    /// AccordoCrowdfund.
    interface Crowdfund {
        struct Campaign {
            address creator;
            address token;
            uint128 targetAmount;
            uint128 raisedAmount;
            uint128 netRaisedAmount;
            uint128 claimedAmount;
            uint128 refundedAmount;
            uint64 createdAt;
            uint64 deadline;
            uint16 feeBps;
            bool claimed;
            bool canceled;
            uint32 contributorsCount;
            bool allOrNothing;
        }
        function createCampaign(bytes32 salt, address token, uint128 targetAmount, uint64 deadline, uint16 feeBps, bool allOrNothing) returns (bytes32);
        function endEarly(bytes32 id);
        function goalReached(bytes32 id) view returns (bool);
        function contributeETH(bytes32 id, string message) payable;
        function contributeERC20(bytes32 id, uint256 amount, string message);
        function claimFunds(bytes32 id);
        function claimFundsTo(bytes32 id, address recipient, uint256 amount);
        function claimableAmount(bytes32 id) view returns (uint256);
        function withdrawalFee(bytes32 id, uint256 amount) view returns (uint256);
        function refund(bytes32 id);
        function cancelCampaign(bytes32 id);
        function getCampaign(bytes32 id) view returns (Campaign c);
        function getContribution(bytes32 id, address donor) view returns (uint256);
        function getCampaignState(bytes32 id) view returns (uint8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_sol_types::SolCall;

    /// Every function this app calls exists, with this exact signature, in the compiled contract.
    #[test]
    fn selectors_exist_in_compiled_abis() {
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/out");
        if !out.exists() {
            eprintln!("contracts/out missing (forge build) — skipped");
            return;
        }
        let selectors_of = |file: &str| -> Vec<String> {
            let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out.join(file)).expect(file)).expect("json");
            v["methodIdentifiers"].as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default()
        };
        let check = |file: &str, sigs: &[&str]| {
            let have = selectors_of(file);
            for s in sigs {
                assert!(have.iter().any(|h| h == s), "{file}: no {s}");
            }
        };
        check("AccordoCheques.sol/AccordoCheques.json", &[
            AccordoCheques::InitChequeCall::SIGNATURE, AccordoCheques::InitSwapChequeCall::SIGNATURE, AccordoCheques::CashOutChequeCall::SIGNATURE,
            AccordoCheques::CashOutSwapChequeCall::SIGNATURE, AccordoCheques::RefundChequeCall::SIGNATURE, AccordoCheques::RefundSwapChequeCall::SIGNATURE,
            AccordoCheques::ExtendChequeCall::SIGNATURE, AccordoCheques::ExtendSwapChequeCall::SIGNATURE, AccordoCheques::getChequeInfoCall::SIGNATURE,
            AccordoCheques::getSwapDetailCall::SIGNATURE, AccordoCheques::getFeeCall::SIGNATURE, AccordoCheques::treasuryCall::SIGNATURE,
        ]);
        check("AccordoTask.sol/AccordoTask.json", &[
            AccordoTask::createChequeCall::SIGNATURE, AccordoTask::claimCall::SIGNATURE, AccordoTask::refundCall::SIGNATURE,
            AccordoTask::extendCall::SIGNATURE, AccordoTask::isClaimedCall::SIGNATURE, AccordoTask::getChequeCall::SIGNATURE,
        ]);
        check("AccordoDrop.sol/AccordoDrop.json", &[
            AccordoDrop::InitMerkleChequeCall::SIGNATURE, AccordoDrop::CashOutMerkleChequeCall::SIGNATURE, AccordoDrop::RefundMerkleChequeCall::SIGNATURE,
            AccordoDrop::ExtendMerkleChequeCall::SIGNATURE, AccordoDrop::getMerkleChequeInfoCall::SIGNATURE, AccordoDrop::slotClaimedAmountCall::SIGNATURE,
            AccordoDrop::creationFeeCall::SIGNATURE, AccordoDrop::slotFeeCall::SIGNATURE, AccordoDrop::maxDropFeeCall::SIGNATURE,
        ]);
        check("AccordoInvoice.sol/InvoiceHub.json", &[
            InvoiceHub::createInvoiceCall::SIGNATURE, InvoiceHub::createInvoicesCall::SIGNATURE, InvoiceHub::payETHCall::SIGNATURE,
            InvoiceHub::payERC20Call::SIGNATURE, InvoiceHub::cancelInvoiceCall::SIGNATURE, InvoiceHub::getInvoiceCall::SIGNATURE,
            InvoiceHub::DEFAULT_FEE_BPSCall::SIGNATURE, InvoiceHub::FEE_DENOMINATORCall::SIGNATURE,
        ]);
        check("AccordoCrowdfund.sol/AccordoCrowdfund.json", &[
            Crowdfund::createCampaignCall::SIGNATURE, Crowdfund::endEarlyCall::SIGNATURE, Crowdfund::goalReachedCall::SIGNATURE,
            Crowdfund::contributeETHCall::SIGNATURE, Crowdfund::contributeERC20Call::SIGNATURE, Crowdfund::claimFundsCall::SIGNATURE,
            Crowdfund::claimFundsToCall::SIGNATURE, Crowdfund::claimableAmountCall::SIGNATURE, Crowdfund::withdrawalFeeCall::SIGNATURE,
            Crowdfund::refundCall::SIGNATURE, Crowdfund::cancelCampaignCall::SIGNATURE, Crowdfund::getCampaignCall::SIGNATURE,
            Crowdfund::getContributionCall::SIGNATURE, Crowdfund::getCampaignStateCall::SIGNATURE,
        ]);
    }
}
