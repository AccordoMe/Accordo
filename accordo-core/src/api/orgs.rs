// SPDX-License-Identifier: GPL-3.0-or-later
//! Organizations (the server's routers/organizations.py): a team people belong to, shown on their
//! profile. Creating one comes with a subscription; the owner adds members by username; admins
//! verify a real one.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::{Api, ApiResult};

/// What a profile shows of one of the person's organizations.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct OrgBrief {
    pub id: i64,
    pub name: String,
    pub logo: Option<String>,
    pub verified: bool,
    /// Their title there ("" = none).
    pub title: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct OrgMember {
    pub user_id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
    pub title: String,
    pub owner: bool,
}

/// An organization's card: itself and its people.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct OrgCard {
    pub id: i64,
    pub name: String,
    pub logo: Option<String>,
    pub verified: bool,
    pub description: String,
    pub website: Option<String>,
    pub owner_id: i64,
    /// Most members it may have (the owner counts).
    pub max_members: i64,
    pub members: Vec<OrgMember>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MyOrgs {
    pub orgs: Vec<OrgCard>,
    /// May create one (a subscription of at least `plan`, or an admin).
    pub can_create: bool,
    pub plan: String,
}

pub async fn mine(api: Api) -> ApiResult<MyOrgs> {
    api.get("/api/orgs/mine").await
}

pub async fn delete(api: Api, id: i64) -> ApiResult<Value> {
    api.delete(&format!("/api/orgs/{id}")).await
}

pub async fn add_member(api: Api, id: i64, username: String, title: String) -> ApiResult<OrgCard> {
    api.post(&format!("/api/orgs/{id}/members"), &json!({ "username": username, "title": title })).await
}

/// The owner removes someone — or, for oneself, leaves.
pub async fn remove_member(api: Api, id: i64, user_id: i64) -> ApiResult<Value> {
    api.delete(&format!("/api/orgs/{id}/members/{user_id}")).await
}

// ── admin ──

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AdminOrg {
    pub id: i64,
    pub name: String,
    pub logo: Option<String>,
    pub description: String,
    pub website: Option<String>,
    pub owner_username: String,
    pub members: i64,
    pub max_members: i64,
    pub verified: bool,
    pub created_at: Option<String>,
}

pub async fn admin_list(api: Api) -> ApiResult<Vec<AdminOrg>> {
    api.get("/api/admin/orgs").await
}

/// None = back to the default (10).
pub async fn admin_set_cap(api: Api, id: i64, max_members: Option<i64>) -> ApiResult<Value> {
    api.put(&format!("/api/admin/orgs/{id}/member-cap"), &json!({ "max_members": max_members })).await
}

pub async fn admin_verify(api: Api, id: i64, verified: bool) -> ApiResult<Value> {
    api.put(&format!("/api/admin/orgs/{id}/verify"), &json!({ "verified": verified })).await
}

/// The owner changes its name / description / website / logo (only what's sent). A new name takes
/// the verified check off.
pub async fn update(api: Api, id: i64, fields: Value) -> ApiResult<OrgCard> {
    api.patch(&format!("/api/orgs/{id}"), &fields).await
}

pub async fn create_full(api: Api, fields: Value) -> ApiResult<OrgCard> {
    api.post("/api/orgs", &fields).await
}
