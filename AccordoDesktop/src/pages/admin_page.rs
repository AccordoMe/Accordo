// SPDX-License-Identifier: GPL-3.0-or-later
//! The Admin Console (AdminPage.tsx and components/admin/*) as one desktop window: overview,
//! users (each in its own card window: limits, plans, rooms), invoices, cheques, access &
//! limits (registration, access codes, waitlist), mini-apps review, notifications, reports.
//! The server checks the admin flag on every call; this only shows what it answers.
//!
//! State and what each message does live here; the window is components/admin_view.rs.

use std::collections::HashSet;

use iced::Task;
use serde_json::Value;

use crate::api::admin::{self as api, AccessCodes, AdminOwnedRoom, AdminUser, CampaignReportItem, MessageReportItem, RegistrationLimits, ReportFilter, ResolvedRecipients, RoomReportItem, SentAnnouncement, UserControlPatch, Waitlist};
use crate::api::client::{Api, ApiError};
use crate::api::mini_apps::MiniApp;
use crate::app::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminTab {
    Overview,
    Users,
    Invoices,
    Cheques,
    Access,
    Apps,
    Orgs,
    Notifications,
    Reports,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    Messages,
    Rooms,
    Campaigns,
}

type R<T> = Result<T, String>;

fn e(err: ApiError) -> String {
    err.message
}

#[derive(Debug, Clone)]
pub enum AdminMsg {
    Open,
    Close,
    Tab(AdminTab),
    Search(String),
    Refresh,
    Users(R<Vec<AdminUser>>),
    Invoices(R<Vec<Value>>),
    Cheques(R<Vec<Value>>),
    Online(R<Value>),
    Reg(R<RegistrationLimits>),
    /// Default limits (data/default_limits.json): loaded; a number edited; the developer default; save.
    Defaults(R<api::DefaultLimits>),
    /// (role, limit) — or (3, room cap).
    DefField(usize, usize, String),
    DefDeveloper(bool),
    DefSave,
    /// Something changed on the server: a note, and what to load again.
    Done(R<String>, Vec<AdminTab>),
    ShowJson(Option<Box<Value>>),
    // users
    SetActive(i64, bool),
    SetApiLogin(i64, bool),
    OpenUser(i64),
    CloseUser,
    Rooms(i64, R<Vec<AdminOwnedRoom>>),
    RoomsLimit(i64),
    Groups(String),
    Suggested(i64),
    AppsLimit(String),
    Articles(bool),
    Developer(bool),
    Role(String),
    Plan(usize, String),
    PlanAdd30(usize),
    SaveControl,
    ResetControl,
    Freeze(String, bool),
    RoomCap(String, String),
    SaveRoomCap(String),
    // access
    RegEnabled(bool),
    RegMax(String),
    RegRate(String),
    RegWindow(String),
    RegInvite(String),
    RegGenerate,
    RegSave,
    Codes(R<AccessCodes>),
    CodesFilter(&'static str),
    CodeCount(String),
    CodeFor(String),
    CodesCreate,
    CodeDelete(String),
    Waitlist(R<Waitlist>),
    WaitPick(i64, bool),
    WaitInvite,
    WaitDelete(i64),
    // apps
    Apps(R<Vec<MiniApp>>),
    Orgs(R<Vec<crate::api::orgs::AdminOrg>>),
    /// (org, verified)
    OrgVerify(i64, bool),
    /// An organization's member cap typed; saved.
    OrgCap(i64, String),
    OrgCapSave(i64),
    AppStatus(i64, &'static str),
    AppGrant(i64),
    AppRemove(i64),
    // notifications
    Notes(R<Vec<SentAnnouncement>>),
    NoteTitle(String),
    NoteBody(String),
    NoteLink(String),
    NoteLevel(&'static str),
    NoteAll(bool),
    NoteWho(String),
    NoteResolve,
    NoteResolved(R<ResolvedRecipients>),
    NoteSend,
    NoteRetract(i64),
    // reports
    ReportKind(ReportKind),
    ReportFilter(ReportFilter),
    MsgReports(R<Vec<MessageReportItem>>),
    RoomReports(R<Vec<RoomReportItem>>),
    CampaignReports(R<Vec<CampaignReportItem>>),
    ActMessage(i64, &'static str),
    ActRoom(i64, &'static str),
    ActCampaign(i64, &'static str),
}

pub const PLANS: [&str; 4] = ["basic", "lite", "pro", "enterprise"];

/// The controls of one user's card, as typed (saved with SaveControl).
#[derive(Debug, Clone, Default)]
pub struct Control {
    pub rooms_limit: i64,
    pub groups: String,
    pub suggested: i64,
    pub apps: String,
    pub articles: bool,
    pub developer: bool,
    /// The profile's label ("member" unless set).
    pub role: String,
    /// basic / lite / pro / enterprise paid until, YYYY-MM-DD ("" = none).
    pub plans: [String; 4],
}

impl Control {
    fn of(u: &AdminUser) -> Self {
        let day = |v: &Option<String>| v.as_deref().and_then(crate::components::chat::display::local).map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_default();
        Self {
            rooms_limit: u.public_rooms_limit.unwrap_or(0),
            groups: u.private_groups_limit.unwrap_or(0).to_string(),
            suggested: u.suggested_rooms_limit.unwrap_or(0),
            apps: u.mini_apps_limit.unwrap_or(0).to_string(),
            articles: u.allowed_write_article,
            developer: u.is_developer,
            role: u.role.clone().unwrap_or_else(|| "member".into()),
            plans: [day(&u.basic_until), day(&u.lite_until), day(&u.pro_until), day(&u.enterprise_until)],
        }
    }

    /// Only what changed; None when something typed isn't a number / date.
    fn patch(&self, u: &AdminUser) -> Result<UserControlPatch, String> {
        let saved = Control::of(u);
        let mut p = UserControlPatch::default();
        // An admin's limits come from the default limits file only (the server ignores these).
        if u.is_admin {
            p.public_rooms_limit = None;
        } else if self.rooms_limit != saved.rooms_limit {
            p.public_rooms_limit = Some(self.rooms_limit);
        }
        if self.articles != u.allowed_write_article {
            p.allowed_write_article = Some(self.articles);
        }
        if self.developer != u.is_developer {
            p.is_developer = Some(self.developer);
        }
        if self.role.trim() != saved.role {
            p.role = Some(self.role.trim().to_string());
        }
        let groups: i64 = self.groups.trim().parse().ok().filter(|n| (0..=10_000).contains(n)).ok_or("Private groups: a number up to 10000")?;
        if !u.is_admin && groups.to_string() != saved.groups {
            p.private_groups_limit = Some(groups);
        }
        if !u.is_admin && self.suggested != saved.suggested {
            p.suggested_rooms_limit = Some(self.suggested);
        }
        let apps: i64 = self.apps.trim().parse().ok().filter(|n| (0..=1000).contains(n)).ok_or("Mini-apps: a number up to 1000")?;
        if !u.is_admin && apps.to_string() != saved.apps {
            p.mini_apps_limit = Some(apps);
        }
        for (i, plan) in self.plans.iter().enumerate() {
            let v = plan.trim();
            if v == saved.plans[i] {
                continue;
            }
            let iso = if v.is_empty() {
                String::new()
            } else {
                let d = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").map_err(|_| format!("{}: a date as YYYY-MM-DD", PLANS[i]))?;
                d.and_hms_opt(23, 59, 59).map(|t| t.and_utc().to_rfc3339()).unwrap_or_default()
            };
            match i {
                0 => p.basic_until = Some(iso),
                1 => p.lite_until = Some(iso),
                2 => p.pro_until = Some(iso),
                _ => p.enterprise_until = Some(iso),
            }
        }
        Ok(p)
    }

    pub fn changed(&self, u: &AdminUser) -> bool {
        let s = Control::of(u);
        self.rooms_limit != s.rooms_limit || self.groups != s.groups || self.suggested != s.suggested || self.apps != s.apps || self.articles != s.articles || self.developer != s.developer || self.role.trim() != s.role || self.plans != s.plans
    }
}

pub struct Profile {
    pub user_id: i64,
    pub rooms: Option<R<Vec<AdminOwnedRoom>>>,
    pub control: Control,
    pub caps: std::collections::HashMap<String, String>,
    pub saving: bool,
}

#[derive(Default)]
pub struct RegForm {
    pub enabled: bool,
    pub max_users: String,
    pub rate: String,
    pub window: String,
    pub invite: String,
}

#[derive(Default)]
pub struct Compose {
    pub title: String,
    pub body: String,
    pub link: String,
    pub level: &'static str,
    pub all: bool,
    pub who: String,
    pub resolved: Option<R<ResolvedRecipients>>,
}

pub struct State {
    pub open: bool,
    pub tab: AdminTab,
    pub search: String,
    pub users: Option<R<Vec<AdminUser>>>,
    pub invoices: Option<R<Vec<Value>>>,
    pub cheques: Option<R<Vec<Value>>>,
    pub online: HashSet<i64>,
    pub reg: Option<R<RegistrationLimits>>,
    pub reg_form: RegForm,
    pub defaults: Option<R<api::DefaultLimits>>,
    /// The form, as typed: account limits per role (LIMIT_ROLES × ACCOUNT_LIMITS; "" = as the
    /// user row for Developer, no limit for Admin; "-" = no limit), room caps, the developer default.
    pub def_form: [[String; 4]; 3],
    pub def_rooms: [String; 4],
    pub def_developer: bool,
    pub json: Option<Value>,
    pub profile: Option<Profile>,
    pub codes: Option<R<AccessCodes>>,
    pub codes_filter: &'static str,
    pub code_count: String,
    pub code_for: String,
    pub waitlist: Option<R<Waitlist>>,
    pub picked: HashSet<i64>,
    pub apps: Option<R<Vec<MiniApp>>>,
    pub orgs: Option<R<Vec<crate::api::orgs::AdminOrg>>>,
    pub org_caps: std::collections::HashMap<i64, String>,
    pub notes: Option<R<Vec<SentAnnouncement>>>,
    pub compose: Compose,
    pub report_kind: ReportKind,
    pub report_filter: ReportFilter,
    pub msg_reports: Option<R<Vec<MessageReportItem>>>,
    pub room_reports: Option<R<Vec<RoomReportItem>>>,
    pub campaign_reports: Option<R<Vec<CampaignReportItem>>>,
    /// Open reports of each kind (the Reports badge).
    pub open_counts: [usize; 3],
    pub busy: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            tab: AdminTab::Overview,
            search: String::new(),
            users: None,
            invoices: None,
            cheques: None,
            online: HashSet::new(),
            reg: None,
            reg_form: RegForm::default(),
            defaults: None,
            def_form: Default::default(),
            def_rooms: Default::default(),
            def_developer: false,
            json: None,
            profile: None,
            codes: None,
            codes_filter: "unused",
            code_count: "5".into(),
            code_for: String::new(),
            waitlist: None,
            picked: HashSet::new(),
            apps: None,
            orgs: None,
            org_caps: Default::default(),
            notes: None,
            compose: Compose { level: "info", all: true, ..Default::default() },
            report_kind: ReportKind::Messages,
            report_filter: ReportFilter::Open,
            msg_reports: None,
            room_reports: None,
            campaign_reports: None,
            open_counts: [0; 3],
            busy: false,
        }
    }
}

fn m(x: AdminMsg) -> Message {
    Message::Admin(x)
}

fn done(note: &'static str, reload: Vec<AdminTab>) -> impl Fn(Result<Value, ApiError>) -> Message {
    move |r| m(AdminMsg::Done(r.map(|_| note.to_string()).map_err(e), reload.clone()))
}

impl State {
    pub fn user(&self, id: i64) -> Option<&AdminUser> {
        self.users.as_ref()?.as_ref().ok()?.iter().find(|u| u.id == id)
    }

    fn load(&mut self, tab: AdminTab, api: &Api) -> Task<Message> {
        let a = api.clone();
        match tab {
            AdminTab::Overview | AdminTab::Users => Task::batch([
                Task::perform(api::list_users(a.clone()), |r| m(AdminMsg::Users(r.map_err(e)))),
                Task::perform(api::list_invoices(a.clone()), |r| m(AdminMsg::Invoices(r.map_err(e)))),
                Task::perform(api::list_cheques(a.clone()), |r| m(AdminMsg::Cheques(r.map_err(e)))),
                Task::perform(api::get_online_users(a.clone()), |r| m(AdminMsg::Online(r.map_err(e)))),
                Task::perform(api::get_registration_limits(a.clone()), |r| m(AdminMsg::Reg(r.map_err(e)))),
                self.load_reports_counts(&a),
            ]),
            AdminTab::Invoices => Task::perform(api::list_invoices(a), |r| m(AdminMsg::Invoices(r.map_err(e)))),
            AdminTab::Cheques => Task::perform(api::list_cheques(a), |r| m(AdminMsg::Cheques(r.map_err(e)))),
            AdminTab::Access => Task::batch([
                Task::perform(api::get_registration_limits(a.clone()), |r| m(AdminMsg::Reg(r.map_err(e)))),
                Task::perform(api::get_default_limits(a.clone()), |r| m(AdminMsg::Defaults(r.map_err(e)))),
                Task::perform(api::list_access_codes(a.clone(), self.codes_filter), |r| m(AdminMsg::Codes(r.map_err(e)))),
                Task::perform(api::list_waitlist(a, "all"), |r| m(AdminMsg::Waitlist(r.map_err(e)))),
            ]),
            AdminTab::Apps => Task::perform(crate::api::mini_apps::admin::list(a), |r| m(AdminMsg::Apps(r.map_err(e)))),
            AdminTab::Orgs => Task::perform(crate::api::orgs::admin_list(a), |r| m(AdminMsg::Orgs(r.map_err(e)))),
            AdminTab::Notifications => Task::batch([
                Task::perform(api::list_notifications(a.clone()), |r| m(AdminMsg::Notes(r.map_err(e)))),
                Task::perform(api::list_users(a), |r| m(AdminMsg::Users(r.map_err(e)))),
            ]),
            AdminTab::Reports => self.load_reports(&a),
        }
    }

    fn load_reports(&self, a: &Api) -> Task<Message> {
        let f = self.report_filter;
        match self.report_kind {
            ReportKind::Messages => Task::perform(api::list_reports(a.clone(), f), |r| m(AdminMsg::MsgReports(r.map_err(e)))),
            ReportKind::Rooms => Task::perform(api::list_room_reports(a.clone(), f), |r| m(AdminMsg::RoomReports(r.map_err(e)))),
            ReportKind::Campaigns => Task::perform(api::list_campaign_reports(a.clone(), f), |r| m(AdminMsg::CampaignReports(r.map_err(e)))),
        }
    }

    /// The open reports of every kind, for the badge (only when the list on screen is "open").
    fn load_reports_counts(&self, a: &Api) -> Task<Message> {
        Task::batch([
            Task::perform(api::list_reports(a.clone(), ReportFilter::Open), |r| m(AdminMsg::MsgReports(r.map_err(e)))),
            Task::perform(api::list_room_reports(a.clone(), ReportFilter::Open), |r| m(AdminMsg::RoomReports(r.map_err(e)))),
            Task::perform(api::list_campaign_reports(a.clone(), ReportFilter::Open), |r| m(AdminMsg::CampaignReports(r.map_err(e)))),
        ])
    }

    fn fill_defaults(&mut self, d: &Value) {
        for (r, (role, _)) in api::LIMIT_ROLES.iter().enumerate() {
            let part = d.get(role);
            for (i, (key, _)) in api::ACCOUNT_LIMITS.iter().enumerate() {
                self.def_form[r][i] = match part.and_then(|p| p.get(key)) {
                    Some(Value::Number(n)) => n.to_string(),
                    // null: no limit — the Admin row shows that as empty, Developer as "-".
                    Some(Value::Null) if r == 1 => "-".into(),
                    _ => String::new(),
                };
            }
        }
        for (i, (key, _)) in api::ROOM_LIMITS.iter().enumerate() {
            self.def_rooms[i] = d.get("groups").and_then(|g| g.get(key)).map(|n| n.to_string()).unwrap_or_default();
        }
        self.def_developer = d.get("user").and_then(|u| u.get("developer")).and_then(Value::as_bool).unwrap_or(false);
    }

    fn fill_reg(&mut self, r: &RegistrationLimits) {
        self.reg_form = RegForm { enabled: r.enabled, max_users: r.max_users.to_string(), rate: r.rate_limit_per_ip.to_string(), window: r.rate_limit_window_seconds.to_string(), invite: r.invite_code.clone() };
    }
}

/// What the window asks the app to show (toasts).
pub enum Note {
    None,
    Ok(String),
    Err(String),
}

pub fn update(st: &mut State, msg: AdminMsg, api: Option<Api>) -> (Task<Message>, Note) {
    let Some(api) = api else { return (Task::none(), Note::Err("Sign in with your password to use the Admin Console".into())) };
    let none = |n: Note| (Task::none(), n);
    match msg {
        AdminMsg::Open => {
            st.open = true;
            let t = st.load(st.tab, &api);
            return (t, Note::None);
        }
        AdminMsg::Close => {
            if st.profile.is_some() {
                st.profile = None;
            } else if st.json.is_some() {
                st.json = None;
            } else {
                st.open = false;
            }
        }
        AdminMsg::Tab(t) => {
            st.tab = t;
            st.search.clear();
            st.json = None;
            return (st.load(t, &api), Note::None);
        }
        AdminMsg::Search(q) => st.search = q,
        AdminMsg::Refresh => return (st.load(st.tab, &api), Note::None),
        AdminMsg::Users(r) => {
            if let (Some(p), Ok(list)) = (st.profile.as_mut(), &r) {
                if let Some(u) = list.iter().find(|u| u.id == p.user_id) {
                    if !p.saving {
                        p.control = Control::of(u);
                    }
                }
            }
            st.users = Some(r);
        }
        AdminMsg::Invoices(r) => st.invoices = Some(r),
        AdminMsg::Cheques(r) => st.cheques = Some(r),
        AdminMsg::Online(r) => {
            if let Ok(v) = r {
                st.online = v.get("online_user_ids").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
            }
        }
        AdminMsg::Reg(r) => {
            if let Ok(l) = &r {
                st.fill_reg(l);
            }
            st.reg = Some(r);
        }
        AdminMsg::Done(r, reload) => {
            st.busy = false;
            if let Some(p) = st.profile.as_mut() {
                p.saving = false;
            }
            let tasks: Vec<Task<Message>> = reload.into_iter().map(|t| st.load(t, &api)).collect();
            return (Task::batch(tasks), match r {
                Ok(n) if n.is_empty() => Note::None,
                Ok(n) => Note::Ok(n),
                Err(x) => Note::Err(x),
            });
        }
        AdminMsg::ShowJson(v) => st.json = v.map(|b| *b),
        AdminMsg::SetActive(id, on) => {
            return (Task::perform(api::set_user_active(api, id, on), done(if on { "Account activated" } else { "Account suspended — signed out everywhere" }, vec![AdminTab::Users])), Note::None);
        }
        AdminMsg::SetApiLogin(id, on) => {
            return (Task::perform(api::set_user_api_login(api, id, on), done(if on { "API-key login allowed" } else { "API-key login off" }, vec![AdminTab::Users])), Note::None);
        }
        AdminMsg::OpenUser(id) => {
            let control = st.user(id).map(Control::of).unwrap_or_default();
            st.profile = Some(Profile { user_id: id, rooms: None, control, caps: Default::default(), saving: false });
            return (Task::perform(api::get_user_rooms(api, id), move |r| m(AdminMsg::Rooms(id, r.map_err(e)))), Note::None);
        }
        AdminMsg::CloseUser => st.profile = None,
        AdminMsg::Rooms(id, r) => {
            if let Some(p) = st.profile.as_mut().filter(|p| p.user_id == id) {
                p.rooms = Some(r);
            }
        }
        AdminMsg::RoomsLimit(n) => if let Some(p) = st.profile.as_mut() { p.control.rooms_limit = n.clamp(0, 100) },
        AdminMsg::Groups(v) => if let Some(p) = st.profile.as_mut() { p.control.groups = v.chars().filter(char::is_ascii_digit).take(5).collect() },
        AdminMsg::Suggested(n) => if let Some(p) = st.profile.as_mut() { p.control.suggested = n.clamp(0, 20) },
        AdminMsg::AppsLimit(v) => if let Some(p) = st.profile.as_mut() { p.control.apps = v.chars().filter(char::is_ascii_digit).take(4).collect() },
        AdminMsg::Articles(v) => if let Some(p) = st.profile.as_mut() { p.control.articles = v },
        AdminMsg::Developer(v) => if let Some(p) = st.profile.as_mut() { p.control.developer = v },
        AdminMsg::Role(v) => if let Some(p) = st.profile.as_mut() { p.control.role = v.chars().take(32).collect() },
        AdminMsg::Plan(i, v) => if let Some(p) = st.profile.as_mut() { p.control.plans[i.min(PLANS.len() - 1)] = v.chars().filter(|c| c.is_ascii_digit() || *c == '-').take(10).collect() },
        AdminMsg::PlanAdd30(i) => {
            if let Some(p) = st.profile.as_mut() {
                let today = chrono::Local::now().date_naive();
                let from = chrono::NaiveDate::parse_from_str(p.control.plans[i.min(PLANS.len() - 1)].trim(), "%Y-%m-%d").ok().filter(|d| *d > today).unwrap_or(today);
                p.control.plans[i.min(PLANS.len() - 1)] = (from + chrono::Duration::days(30)).format("%Y-%m-%d").to_string();
            }
        }
        AdminMsg::ResetControl => {
            if let Some(p) = st.profile.as_mut() {
                let id = p.user_id;
                if let Some(u) = st.users.as_ref().and_then(|r| r.as_ref().ok()).and_then(|l| l.iter().find(|u| u.id == id)) {
                    p.control = Control::of(u);
                }
            }
        }
        AdminMsg::SaveControl => {
            let Some(p) = st.profile.as_ref() else { return none(Note::None) };
            let Some(u) = st.user(p.user_id) else { return none(Note::None) };
            let patch = match p.control.patch(u) {
                Ok(x) => x,
                Err(x) => return none(Note::Err(x)),
            };
            let (id, name) = (u.id, u.username.clone());
            if let Some(p) = st.profile.as_mut() {
                p.saving = true;
            }
            let note = format!("Saved @{name}'s permissions");
            return (Task::perform(api::update_user_control(api, id, patch), move |r| m(AdminMsg::Done(r.map(|_| note.clone()).map_err(e), vec![AdminTab::Users]))), Note::None);
        }
        AdminMsg::Freeze(key, frozen) => {
            let id = st.profile.as_ref().map(|p| p.user_id);
            let reload = Task::perform(api::set_room_frozen(api.clone(), key, frozen, None), done(if frozen { "Room frozen — no new members, no settings changes" } else { "Room unfrozen" }, vec![]));
            let rooms = id.map(|id| Task::perform(api::get_user_rooms(api, id), move |r| m(AdminMsg::Rooms(id, r.map_err(e))))).unwrap_or_else(Task::none);
            return (reload.chain(rooms), Note::None);
        }
        AdminMsg::RoomCap(key, v) => if let Some(p) = st.profile.as_mut() { p.caps.insert(key, v.chars().filter(char::is_ascii_digit).take(5).collect()); },
        AdminMsg::SaveRoomCap(key) => {
            let Some(p) = st.profile.as_ref() else { return none(Note::None) };
            let v = p.caps.get(&key).cloned().unwrap_or_default();
            let cap = if v.trim().is_empty() { None } else { v.trim().parse().ok() };
            let id = p.user_id;
            let save = Task::perform(api::set_room_member_cap(api.clone(), key, cap), done("Member cap saved", vec![]));
            return (save.chain(Task::perform(api::get_user_rooms(api, id), move |r| m(AdminMsg::Rooms(id, r.map_err(e))))), Note::None);
        }
        // ── access ──
        AdminMsg::RegEnabled(v) => {
            st.reg_form.enabled = v;
            return save_reg(st, api, if v { "Registration is now OPEN!" } else { "Registration is now CLOSED!" });
        }
        AdminMsg::RegMax(v) => st.reg_form.max_users = digits(v, 7),
        AdminMsg::RegRate(v) => st.reg_form.rate = digits(v, 5),
        AdminMsg::RegWindow(v) => st.reg_form.window = digits(v, 7),
        AdminMsg::RegInvite(v) => st.reg_form.invite = v.trim().to_string(),
        AdminMsg::RegGenerate => {
            let mut b = [0u8; 6];
            let _ = getrandom::fill(&mut b);
            const A: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
            st.reg_form.invite = format!("ACCORDO-{}", b.iter().map(|x| A[*x as usize % A.len()] as char).collect::<String>());
        }
        AdminMsg::RegSave => return save_reg(st, api, "Registration limits saved"),
        AdminMsg::Defaults(r) => {
            if let Ok(d) = &r {
                st.fill_defaults(d);
            }
            st.defaults = Some(r);
        }
        AdminMsg::DefField(role, i, v) => {
            // Digits, or "-" (no limit) in the Developer / Admin rows.
            let v = if role > 0 && role < 3 && v.trim() == "-" { "-".to_string() } else { digits(v, 9) };
            match role {
                0..=2 => st.def_form[role][i] = v,
                _ => st.def_rooms[i] = v,
            }
        }
        AdminMsg::DefDeveloper(v) => st.def_developer = v,
        AdminMsg::DefSave => {
            return match defaults_body(st) {
                Ok(body) => (Task::perform(api::update_default_limits(api, body), |r| m(AdminMsg::Done(r.map(|_| "Default limits saved".to_string()).map_err(e), vec![AdminTab::Access]))), Note::None),
                Err(msg) => (Task::none(), Note::Err(msg)),
            };
        }
        AdminMsg::Codes(r) => st.codes = Some(r),
        AdminMsg::CodesFilter(f) => {
            st.codes_filter = f;
            return (Task::perform(api::list_access_codes(api, f), |r| m(AdminMsg::Codes(r.map_err(e)))), Note::None);
        }
        AdminMsg::CodeCount(v) => st.code_count = digits(v, 3),
        AdminMsg::CodeFor(v) => st.code_for = v.trim().trim_start_matches('@').to_string(),
        AdminMsg::CodesCreate => {
            let n: i64 = st.code_count.parse().unwrap_or(0);
            if !(1..=100).contains(&n) {
                return none(Note::Err("Make 1 to 100 codes at a time".into()));
            }
            let who = Some(st.code_for.clone()).filter(|w| !w.is_empty());
            return (Task::perform(api::create_access_codes(api, n, who), done("Access codes created", vec![AdminTab::Access])), Note::None);
        }
        AdminMsg::CodeDelete(c) => return (Task::perform(api::delete_access_code(api, c), done("Code deleted", vec![AdminTab::Access])), Note::None),
        AdminMsg::Waitlist(r) => st.waitlist = Some(r),
        AdminMsg::WaitPick(id, on) => {
            if on {
                st.picked.insert(id);
            } else {
                st.picked.remove(&id);
            }
        }
        AdminMsg::WaitInvite => {
            let ids: Vec<i64> = st.picked.drain().collect();
            if ids.is_empty() {
                return none(Note::Err("Tick the people to invite".into()));
            }
            return (
                Task::perform(api::invite_from_waitlist(api, ids), |r| {
                    m(AdminMsg::Done(r.map(|w| format!("Invited {}{}", w.invited.len(), if w.emailed { " — codes emailed" } else { " — email is off: send them the codes yourself" })).map_err(e), vec![AdminTab::Access]))
                }),
                Note::None,
            );
        }
        AdminMsg::WaitDelete(id) => return (Task::perform(api::delete_waitlist_entry(api, id), done("Removed from the waitlist", vec![AdminTab::Access])), Note::None),
        // ── apps ──
        AdminMsg::Apps(r) => st.apps = Some(r),
        AdminMsg::Orgs(r) => {
            st.org_caps.clear();
            st.orgs = Some(r);
        }
        AdminMsg::OrgCap(id, v) => {
            st.org_caps.insert(id, digits(v, 6));
        }
        AdminMsg::OrgCapSave(id) => {
            let cap = st.org_caps.get(&id).and_then(|v| v.parse::<i64>().ok()).filter(|n| *n >= 1);
            return (Task::perform(crate::api::orgs::admin_set_cap(api, id, cap), |r| m(AdminMsg::Done(r.map(|_| "Member cap saved".to_string()).map_err(e), vec![AdminTab::Orgs]))), Note::None);
        }
        AdminMsg::OrgVerify(id, v) => {
            let note = if v { "Organization verified" } else { "Verification taken off" };
            return (Task::perform(crate::api::orgs::admin_verify(api, id, v), move |r| m(AdminMsg::Done(r.map(|_| note.to_string()).map_err(e), vec![AdminTab::Orgs]))), Note::None);
        }
        AdminMsg::AppStatus(id, s) => {
            let note = match s {
                "active" => "App approved",
                "rejected" => "App rejected",
                _ => "App switched off",
            };
            return (Task::perform(crate::api::mini_apps::admin::set_status(api, id, s, None), move |r| m(AdminMsg::Done(r.map(|_| note.to_string()).map_err(e), vec![AdminTab::Apps]))), Note::None);
        }
        AdminMsg::AppGrant(id) => return (Task::perform(crate::api::mini_apps::admin::grant_year(api, id), |r| m(AdminMsg::Done(r.map(|a| format!("A year on the house for {}", a.name)).map_err(e), vec![AdminTab::Apps]))), Note::None),
        AdminMsg::AppRemove(id) => return (Task::perform(crate::api::mini_apps::admin::remove(api, id), done("App deleted", vec![AdminTab::Apps])), Note::None),
        // ── notifications ──
        AdminMsg::Notes(r) => st.notes = Some(r),
        AdminMsg::NoteTitle(v) => st.compose.title = v.chars().take(120).collect(),
        AdminMsg::NoteBody(v) => st.compose.body = v.chars().take(2000).collect(),
        AdminMsg::NoteLink(v) => st.compose.link = v.trim().to_string(),
        AdminMsg::NoteLevel(l) => st.compose.level = l,
        AdminMsg::NoteAll(v) => st.compose.all = v,
        AdminMsg::NoteWho(v) => {
            st.compose.who = v;
            st.compose.resolved = None;
        }
        AdminMsg::NoteResolve => {
            let ids = identifiers(&st.compose.who);
            if ids.is_empty() {
                return none(Note::Err("Type the usernames, emails or IDs".into()));
            }
            return (Task::perform(api::resolve_recipients(api, ids), |r| m(AdminMsg::NoteResolved(r.map_err(e)))), Note::None);
        }
        AdminMsg::NoteResolved(r) => st.compose.resolved = Some(r),
        AdminMsg::NoteSend => {
            let c = &st.compose;
            if c.title.trim().is_empty() || c.body.trim().is_empty() {
                return none(Note::Err("Give it a title and a message".into()));
            }
            let ids = identifiers(&c.who);
            if !c.all && ids.is_empty() {
                return none(Note::Err("Pick who gets it".into()));
            }
            let payload = crate::api::admin::SendAnnouncementPayload {
                title: c.title.trim().to_string(),
                body: c.body.trim().to_string(),
                level: c.level.to_string(),
                link: Some(Some(c.link.clone()).filter(|l| !l.is_empty())),
                target_type: if c.all { "all".into() } else { "users".into() },
                identifiers: Some(if c.all { Vec::new() } else { ids }),
            };
            st.busy = true;
            st.compose.title.clear();
            st.compose.body.clear();
            st.compose.link.clear();
            return (Task::perform(api::send_notification(api, payload), |r| m(AdminMsg::Done(r.map(|a| format!("Sent to {} user{}", a.recipients_count, if a.recipients_count == 1 { "" } else { "s" })).map_err(e), vec![AdminTab::Notifications]))), Note::None);
        }
        AdminMsg::NoteRetract(id) => return (Task::perform(api::retract_notification(api, id), |r| m(AdminMsg::Done(r.map(|_| "Notification retracted".to_string()).map_err(e), vec![AdminTab::Notifications]))), Note::None),
        // ── reports ──
        AdminMsg::ReportKind(k) => {
            st.report_kind = k;
            return (st.load_reports(&api), Note::None);
        }
        AdminMsg::ReportFilter(f) => {
            st.report_filter = f;
            return (st.load_reports(&api), Note::None);
        }
        AdminMsg::MsgReports(r) => {
            if let Ok(l) = &r {
                if st.report_filter == ReportFilter::Open || st.report_kind != ReportKind::Messages {
                    st.open_counts[0] = l.iter().filter(|x| x.status == "open").map(|x| x.message_id).collect::<HashSet<_>>().len();
                }
            }
            if st.report_kind == ReportKind::Messages {
                st.msg_reports = Some(r);
            }
        }
        AdminMsg::RoomReports(r) => {
            if let Ok(l) = &r {
                if st.report_filter == ReportFilter::Open || st.report_kind != ReportKind::Rooms {
                    st.open_counts[1] = l.iter().filter(|x| x.status == "open").map(|x| x.room_key.clone()).collect::<HashSet<_>>().len();
                }
            }
            if st.report_kind == ReportKind::Rooms {
                st.room_reports = Some(r);
            }
        }
        AdminMsg::CampaignReports(r) => {
            if let Ok(l) = &r {
                if st.report_filter == ReportFilter::Open || st.report_kind != ReportKind::Campaigns {
                    st.open_counts[2] = l.iter().filter(|x| x.status == "open").map(|x| x.campaign_id.clone()).collect::<HashSet<_>>().len();
                }
            }
            if st.report_kind == ReportKind::Campaigns {
                st.campaign_reports = Some(r);
            }
        }
        AdminMsg::ActMessage(id, a) => return (Task::perform(api::act_on_report(api, id, a), move |r| m(AdminMsg::Done(r.map(|x| moderation_note(a, x.resolved_reports)).map_err(e), vec![AdminTab::Reports]))), Note::None),
        AdminMsg::ActRoom(id, a) => return (Task::perform(api::act_on_room_report(api, id, a), move |r| m(AdminMsg::Done(r.map(|x| moderation_note(a, x.resolved_reports)).map_err(e), vec![AdminTab::Reports]))), Note::None),
        AdminMsg::ActCampaign(id, a) => return (Task::perform(api::act_on_campaign_report(api, id, a, None), move |r| m(AdminMsg::Done(r.map(|x| moderation_note(a, x.resolved_reports)).map_err(e), vec![AdminTab::Reports]))), Note::None),
    }
    (Task::none(), Note::None)
}

fn moderation_note(action: &str, n: i64) -> String {
    if action == "dismiss" { "Report dismissed".into() } else { format!("Done — {n} report{} resolved", if n == 1 { "" } else { "s" }) }
}

fn digits(v: String, max: usize) -> String {
    v.chars().filter(char::is_ascii_digit).take(max).collect()
}

/// "alice, @bob\n42 carol@x.io" → ["alice", "bob", "42", "carol@x.io"].
pub fn identifiers(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).map(|w| w.trim().trim_start_matches('@')).filter(|w| !w.is_empty()) {
        if !out.iter().any(|x| x.eq_ignore_ascii_case(w)) {
            out.push(w.to_string());
        }
    }
    out
}

/// The default limits form → the file's shape for PUT /api/admin/default-limits.
fn defaults_body(st: &State) -> Result<Value, String> {
    let mut out = serde_json::Map::new();
    for (r, (role, role_label)) in api::LIMIT_ROLES.iter().enumerate() {
        let mut part = serde_json::Map::new();
        for (i, (key, label)) in api::ACCOUNT_LIMITS.iter().enumerate() {
            let v = st.def_form[r][i].trim();
            let value: Value = match (r, v) {
                (0, "") => return Err(format!("User · {label}: a whole number")),
                (1, "") => "inherit".into(),
                (_, "" | "-") => Value::Null,
                (_, n) => n.parse::<i64>().map(Value::from).map_err(|_| format!("{role_label} · {label}: a whole number"))?,
            };
            part.insert(key.to_string(), value);
        }
        if r == 0 {
            part.insert("developer".into(), st.def_developer.into());
        }
        out.insert(role.to_string(), Value::Object(part));
    }
    let mut groups = serde_json::Map::new();
    for (i, (key, label)) in api::ROOM_LIMITS.iter().enumerate() {
        let n = st.def_rooms[i].trim().parse::<i64>().map_err(|_| format!("{label}: a whole number"))?;
        groups.insert(key.to_string(), n.into());
    }
    out.insert("groups".into(), Value::Object(groups));
    Ok(Value::Object(out))
}

fn save_reg(st: &mut State, api: Api, note: &'static str) -> (Task<Message>, Note) {
    let f = &st.reg_form;
    let num = |v: &str| v.trim().parse::<i64>().ok();
    let (Some(max), Some(rate), Some(window)) = (num(&f.max_users), num(&f.rate), num(&f.window)) else {
        return (Task::none(), Note::Err("Limits: whole numbers only".into()));
    };
    let payload = crate::api::admin::UpdateRegistrationLimitsPayload { enabled: f.enabled, max_users: max, rate_limit_per_ip: rate, rate_limit_window_seconds: window, invite_code: Some(Some(f.invite.clone()).filter(|c| !c.is_empty())) };
    (Task::perform(api::update_registration_limits(api, payload), move |r| m(AdminMsg::Done(r.map(|_| note.to_string()).map_err(e), vec![AdminTab::Access]))), Note::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipients_are_split() {
        assert_eq!(identifiers("alice, @bob\n42  carol@x.io; Alice"), vec!["alice", "bob", "42", "carol@x.io"]);
    }

    #[test]
    fn control_patch_has_only_changes() {
        let u = AdminUser { id: 1, public_rooms_limit: Some(2), private_groups_limit: Some(50), mini_apps_limit: Some(10), ..Default::default() };
        let mut c = Control::of(&u);
        assert!(!c.changed(&u));
        c.rooms_limit = 3;
        c.plans[2] = "2027-01-31".into();
        let p = c.patch(&u).unwrap();
        assert_eq!(p.public_rooms_limit, Some(3));
        assert!(p.private_groups_limit.is_none());
        assert!(p.pro_until.unwrap().starts_with("2027-01-31T23:59:59"));
        c.groups = "x".into();
        assert!(c.patch(&u).is_err());
        // Clearing a plan sends "" (removes it).
        let u2 = AdminUser { basic_until: Some("2027-03-01T10:00:00Z".into()), private_groups_limit: Some(50), mini_apps_limit: Some(10), ..Default::default() };
        let mut c2 = Control::of(&u2);
        c2.plans[0].clear();
        assert_eq!(c2.patch(&u2).unwrap().basic_until.as_deref(), Some(""));
    }
}
