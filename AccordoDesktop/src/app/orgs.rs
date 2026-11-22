// SPDX-License-Identifier: GPL-3.0-or-later
//! Organizations on the rail: the ones I'm in under the menu (a click opens one's window), "+"
//! for a new one (logo, name, description, website). The window shows the people with their
//! titles; the owner adds them by username, removes them, changes the logo, deletes it; anyone
//! leaves. Creating one comes with a subscription (Enterprise) — or an admin.

use crate::app::{DataMsg};
use iced::Task;
use serde_json::{json, Map, Value};

use super::{App, Message};
use crate::api::orgs::{self, MyOrgs};
use crate::components::file_picker::{PickerMsg, Purpose};
use crate::components::toast::Kind;

#[derive(Debug, Clone)]
pub enum OrgMsg {
    Loaded(Result<MyOrgs, String>),
    /// "+": the new-organization window.
    New,
    /// An organization's window.
    Open(i64),
    Close,
    Name(String),
    Description(String),
    Website(String),
    /// Pick a logo file: for the new one (None) or an organization of mine.
    PickLogo(Option<i64>),
    /// A picked logo, as a data URL.
    Logo(String),
    ClearLogo,
    Create,
    AddName(String),
    AddTitle(String),
    Add(i64),
    /// (org, user): the owner removes them, or oneself leaves.
    Remove(i64, i64),
    Delete(i64),
    /// A change went through (a note; `close`: the window goes) or didn't.
    Done(Result<String, String>, bool),
}

#[derive(Debug, Clone, Default)]
pub struct NewOrg {
    pub name: String,
    pub description: String,
    pub website: String,
    pub logo: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Window {
    New(NewOrg),
    /// An organization (by id) and what's typed to add someone (username, title).
    Org(i64, String, String),
}

#[derive(Default)]
pub struct Orgs {
    /// None until loaded (or signed out).
    pub mine: Option<MyOrgs>,
    pub window: Option<Window>,
    pub busy: bool,
    /// The organization whose logo is being picked (None: the new one's).
    logo_for: Option<i64>,
}

fn reload(api: crate::api::client::Api) -> Task<Message> {
    Task::perform(orgs::mine(api), |r| Message::Org(OrgMsg::Loaded(r.map_err(|e| e.message))))
}

impl App {
    /// After signing in: the rail's organizations.
    pub(crate) fn load_orgs(&mut self) -> Task<Message> {
        self.orgs = Orgs::default();
        match self.session.as_ref().filter(|s| !s.token.is_empty()) {
            Some(s) => reload(s.api()),
            None => Task::none(),
        }
    }

    pub(crate) fn org_update(&mut self, m: OrgMsg) -> Task<Message> {
        let Some(session) = self.session.as_ref() else { return Task::none() };
        let api = session.api();
        let me = session.user.id;
        let st = &mut self.orgs;
        let done = |close: bool, note: String| move |r: Result<Value, String>| Message::Org(OrgMsg::Done(r.map(|_| note.clone()), close));
        match m {
            OrgMsg::Loaded(r) => st.mine = Some(r.unwrap_or_default()),
            OrgMsg::New => st.window = Some(Window::New(NewOrg::default())),
            OrgMsg::Open(id) => st.window = Some(Window::Org(id, String::new(), String::new())),
            OrgMsg::Close => st.window = None,
            OrgMsg::Name(v) => {
                if let Some(Window::New(f)) = st.window.as_mut() {
                    f.name = v.chars().take(64).collect();
                }
            }
            OrgMsg::Description(v) => {
                if let Some(Window::New(f)) = st.window.as_mut() {
                    f.description = v.chars().take(300).collect();
                }
            }
            OrgMsg::Website(v) => {
                if let Some(Window::New(f)) = st.window.as_mut() {
                    f.website = v.trim().chars().take(200).collect();
                }
            }
            OrgMsg::PickLogo(target) => {
                st.logo_for = target;
                return Task::done(Message::Picker(PickerMsg::Open(Purpose::OrgLogo)));
            }
            OrgMsg::Logo(url) => match st.logo_for {
                None => {
                    if let Some(Window::New(f)) = st.window.as_mut() {
                        f.logo = Some(url);
                    }
                }
                Some(id) if !st.busy => {
                    st.busy = true;
                    let task = Task::perform(orgs::update(api, id, json!({ "logo": url })), |r| r.map(|_| Value::Null).map_err(|e| e.message));
                    return task.map(done(false, "Logo changed".into()));
                }
                Some(_) => {}
            },
            OrgMsg::ClearLogo => {
                if let Some(Window::New(f)) = st.window.as_mut() {
                    f.logo = None;
                }
            }
            OrgMsg::Create if !st.busy => {
                let Some(Window::New(f)) = st.window.as_ref() else { return Task::none() };
                let name = f.name.trim().to_string();
                if name.chars().count() < 2 {
                    return self.toasts.show(Kind::Error, "The name: at least 2 characters");
                }
                if !f.website.is_empty() && !f.website.starts_with("https://") {
                    return self.toasts.show(Kind::Error, "The website: an https:// link");
                }
                let mut body = Map::new();
                body.insert("name".into(), name.clone().into());
                if !f.description.trim().is_empty() {
                    body.insert("description".into(), f.description.trim().into());
                }
                if !f.website.is_empty() {
                    body.insert("website".into(), f.website.clone().into());
                }
                if let Some(l) = &f.logo {
                    body.insert("logo".into(), l.clone().into());
                }
                st.busy = true;
                let task = Task::perform(orgs::create_full(api, Value::Object(body)), |r| r.map(|c| json!(c.id)).map_err(|e| e.message));
                return task.map(done(true, format!("Created {name}")));
            }
            OrgMsg::AddName(v) => {
                if let Some(Window::Org(_, who, _)) = st.window.as_mut() {
                    *who = v.chars().take(64).collect();
                }
            }
            OrgMsg::AddTitle(v) => {
                if let Some(Window::Org(_, _, title)) = st.window.as_mut() {
                    *title = v.chars().take(48).collect();
                }
            }
            OrgMsg::Add(id) if !st.busy => {
                let Some(Window::Org(_, who, title)) = st.window.as_ref() else { return Task::none() };
                let who = who.trim().trim_start_matches('@').to_string();
                if who.is_empty() {
                    return self.toasts.show(Kind::Error, "Type their username");
                }
                st.busy = true;
                let note = format!("Added @{who}");
                let task = Task::perform(orgs::add_member(api, id, who, title.trim().to_string()), |r| r.map(|_| Value::Null).map_err(|e| e.message));
                return task.map(done(false, note));
            }
            OrgMsg::Remove(id, user_id) if !st.busy => {
                st.busy = true;
                let leaving = user_id == me;
                let task = Task::perform(orgs::remove_member(api, id, user_id), |r| r.map_err(|e| e.message));
                return task.map(done(leaving, if leaving { "You left the organization".into() } else { "Removed".into() }));
            }
            OrgMsg::Delete(id) if !st.busy => {
                st.busy = true;
                let task = Task::perform(orgs::delete(api, id), |r| r.map_err(|e| e.message));
                return task.map(done(true, "Organization deleted".into()));
            }
            OrgMsg::Create | OrgMsg::Add(_) | OrgMsg::Remove(..) | OrgMsg::Delete(_) => {}
            OrgMsg::Done(r, close) => {
                st.busy = false;
                let toast = match r {
                    Ok(note) => {
                        match st.window.as_mut() {
                            Some(Window::Org(_, who, title)) => {
                                who.clear();
                                title.clear();
                            }
                            _ if close => {}
                            _ => {}
                        }
                        if close {
                            st.window = None;
                        }
                        self.toasts.show(Kind::Success, note)
                    }
                    Err(e) => self.toasts.show(Kind::Error, e),
                };
                // Fresh lists, and /me (the profile shows them).
                return Task::batch([toast, reload(api.clone()), Task::perform(crate::api::auth::me(api), |v| Message::Data(DataMsg::UserRefreshed(v)))]);
            }
        }
        Task::none()
    }

    /// A picked logo file, made into a data URL (AvatarMsg::Ready for Purpose::OrgLogo).
    pub(crate) fn org_logo_picked(&mut self, r: Result<String, String>) -> Task<Message> {
        match r {
            Ok(url) => Task::done(Message::Org(OrgMsg::Logo(url))),
            Err(e) => self.toasts.show(Kind::Error, e),
        }
    }
}
