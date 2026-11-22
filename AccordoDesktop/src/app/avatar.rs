// SPDX-License-Identifier: GPL-3.0-or-later
//! The account's avatar: the avatar card in Settings and the profile menu — editing the URL,
//! uploading a picture, saving it to the profile (the API call, and the session the new value
//! lands in).
//!
//! Responsible for: `AvatarMsg` and the account's own picture (`session.user.avatar`). Not here:
//! a room's picture (`components/chat/manage_room.rs`, `room_avatar.rs`), and drawing an avatar
//! anywhere (`components/ui.rs`, `avatar_pic`).

use super::*;

impl App {
    /// The account's picture (SettingsPage.tsx handleSaveAvatar): editing the URL, uploading a
    /// file, saving it to the profile.
    pub(super) fn avatar_update(&mut self, m: AvatarMsg) -> Task<Message> {
        let Some(session) = self.session.as_ref() else { return Task::none() };
        let api = session.api();
        let save = |value: String, done: &'static str| {
            // "" resets: the server keeps nothing (a null would mean "no change").
            let update = crate::api::auth::ProfileUpdate { avatar: Some(Some(value)), ..Default::default() };
            Task::perform(crate::api::auth::update_profile(api.clone(), update), move |r| Message::Avatar(AvatarMsg::Saved(r.map(Box::new).map_err(|e| e.message), done)))
        };
        let st = &mut self.settings;
        match m {
            AvatarMsg::Edit => {
                st.avatar_edit = match st.avatar_edit {
                    Some(_) => None,
                    // A data URL (an uploaded photo) isn't something to edit as text.
                    None => Some(session.user.avatar.clone().filter(|a| !a.starts_with("data:")).unwrap_or_default()),
                };
            }
            AvatarMsg::Url(v) => st.avatar_edit = Some(v),
            AvatarMsg::Upload => return Task::done(Message::Picker(PickerMsg::Open(Purpose::ProfileAvatar))),
            AvatarMsg::Save if !st.avatar_saving => {
                let v = st.avatar_edit.clone().unwrap_or_default().trim().to_string();
                if !v.is_empty() && !v.starts_with("https://") {
                    return self.toasts.show(Kind::Error, "Use an https:// link to a picture — or upload a photo");
                }
                st.avatar_saving = true;
                return save(v.clone(), if v.is_empty() { "Avatar reset to default" } else { "Avatar updated" });
            }
            AvatarMsg::Reset if !st.avatar_saving => {
                st.avatar_saving = true;
                return save(String::new(), "Avatar reset to default");
            }
            AvatarMsg::Save | AvatarMsg::Reset => {}
            AvatarMsg::Ready(Purpose::OrgLogo, r) => return self.org_logo_picked(r),
            AvatarMsg::Ready(Purpose::RoomAvatar, Ok(url)) => {
                return Task::done(Message::Chat(chat_page::ChatMsg::Manage(crate::components::chat::manage_room::ManageMsg::Avatar(Some(url)))));
            }
            AvatarMsg::Ready(_, Ok(url)) if !st.avatar_saving => {
                st.avatar_saving = true;
                return save(url, "Avatar updated");
            }
            AvatarMsg::Ready(_, Ok(_)) => {}
            AvatarMsg::Ready(_, Err(e)) => return self.toasts.show(Kind::Error, e),
            AvatarMsg::Saved(r, done) => {
                st.avatar_saving = false;
                return match r {
                    Ok(user) => {
                        st.avatar_edit = None;
                        let mut save = Task::none();
                        if let Some(s) = self.session.as_mut() {
                            s.user = *user;
                            if self.store.set_account_avatar(s.user.id, &s.base, s.user.picture()) {
                                save = self.save_store();
                            }
                        }
                        Task::batch([save, self.toasts.show(Kind::Success, done)])
                    }
                    Err(e) => self.toasts.show(Kind::Error, format!("Couldn't save the avatar: {e}")),
                };
            }
        }
        Task::none()
    }
}
