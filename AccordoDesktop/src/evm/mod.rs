// SPDX-License-Identifier: GPL-3.0-or-later
//! EVM contracts from the desktop (the web's utils/*Contract.ts, drops.ts, taskCheques.ts), signed
//! with the Accordo wallet's own key. Solana / TON / Tron aren't here.
// The pages call into this step by step (cheques first); until each is wired, parts are unused.
#![allow(dead_code)]

pub mod abi;
pub mod balances;
pub mod cheques;
pub mod client;
pub mod crowdfund;
pub mod drops;
pub mod errors;
pub mod invoices;

#[cfg(test)]
mod tests;
