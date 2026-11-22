// SPDX-License-Identifier: GPL-3.0-or-later
//! Talking to the Accordo backend — the web app's src/api/, module for module:
//!
//! | here            | web (src/api)        |
//! |-----------------|----------------------|
//! | client          | client.ts            |
//! | auth            | auth.ts              |
//! | chat            | chat.ts              |
//! | cheques         | cheques.ts           |
//! | invoices        | invoices.ts          |
//! | crowdfund       | crowdfund.ts         |
//! | nft             | nft.ts               |
//! | tasks           | tasks.ts             |
//! | mini_apps       | miniApps.ts          |
//! | admin           | admin.ts             |
//! | orgs            | (organizations)      |
//! | stickers        | stickers.ts          |
//! | gifs            | gifs.ts              |
//! | articles        | articles.ts          |
//! | webhooks        | webhooks.ts          |
//! | audit_log       | auditLog.ts          |
//! | dashboard       | dashboard.ts         |
//! | contracts       | contracts.ts         |
//! | reconcile       | reconcile.ts         |
//! | account         | (SessionsCard.tsx)   |
//! | data            | (Web3Context, TokenModal: /api/data) |
//! | session_store   | (AuthContext: the token in localStorage) |
//! | socket          | (context/SocketContext.tsx: the live connection) |
//!
//! queries.ts isn't here: it's React Query's cache, not API calls; its list endpoints are
//! `cheques::incoming / mine`, `invoices::incoming / mine`, `crowdfund::active / get_my`.
//!
//! Ported ahead of the screens that will use them, hence the allow below.

#![allow(dead_code)]

pub mod account;
pub mod admin;
pub mod articles;
pub mod audit_log;
pub mod auth;
pub mod chat;
pub mod cheques;
pub mod client;
pub mod contracts;
pub mod crowdfund;
pub mod dashboard;
pub mod drops;
pub mod data;
pub mod e2ee;
pub mod gifs;
pub mod invoices;
pub mod mini_apps;
pub mod nft;
pub mod orgs;
pub mod reconcile;
pub mod session_store;
pub mod socket;
pub mod stickers;
pub mod tasks;
pub mod webhooks;
