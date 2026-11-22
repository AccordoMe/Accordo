// SPDX-License-Identifier: GPL-3.0-or-later
//! The user's room list for the whole app, not just the chat page (store/useRoomsStore.ts): the
//! socket keeps it current, so the chat opens on an up-to-date list after time on another page.
//!
//! Overlapping loads share one request — a burst of socket events costs one fetch: call
//! [`RoomsStore::begin_load`] before fetching and [`RoomsStore::finish_load`] with the answer; when
//! the latter says so, fetch once more (something asked for a reload meanwhile).

use crate::api::chat::ChatRoomItem;

#[derive(Debug, Default)]
pub struct RoomsStore {
    pub rooms: Vec<ChatRoomItem>,
    /// True once the server's list has arrived at least once this session.
    pub loaded: bool,
    inflight: bool,
    again: bool,
}

impl RoomsStore {
    /// True: start a request now. False: one is running — it'll be repeated when it ends.
    pub fn begin_load(&mut self) -> bool {
        if self.inflight {
            self.again = true;
            return false;
        }
        self.inflight = true;
        self.again = false;
        true
    }

    /// Takes the answer (a failure keeps the old list). True: load again right away.
    pub fn finish_load<E>(&mut self, result: Result<Vec<ChatRoomItem>, E>) -> bool {
        if let Ok(list) = result {
            self.rooms = list;
            self.loaded = true;
        }
        if self.again {
            self.again = false;
            return true; // still in flight: the repeat request
        }
        self.inflight = false;
        false
    }

    pub fn get(&self, room_key: &str) -> Option<&ChatRoomItem> {
        self.rooms.iter().find(|r| r.room_key == room_key)
    }

    /// Changes one room in place.
    pub fn update_room(&mut self, room_key: &str, update: impl FnOnce(&mut ChatRoomItem)) {
        if let Some(room) = self.rooms.iter_mut().find(|r| r.room_key == room_key) {
            update(room);
        }
    }

    /// Adds a room (just created) or replaces the one with its key.
    pub fn upsert(&mut self, room: ChatRoomItem) {
        match self.rooms.iter_mut().find(|r| r.room_key == room.room_key) {
            Some(existing) => *existing = room,
            None => self.rooms.push(room),
        }
    }

    /// Applies a change to every room at once (e.g. a member's new avatar).
    pub fn map_rooms(&mut self, update: impl FnMut(&mut ChatRoomItem)) {
        self.rooms.iter_mut().for_each(update);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(key: &str) -> ChatRoomItem {
        ChatRoomItem { room_key: key.into(), ..Default::default() }
    }

    #[test]
    fn overlapping_loads_share_one_request() {
        let mut s = RoomsStore::default();
        assert!(s.begin_load());
        assert!(!s.begin_load()); // a socket event while loading
        assert!(s.finish_load::<()>(Ok(vec![room("a")]))); // → once more
        assert!(!s.finish_load::<()>(Ok(vec![room("a"), room("b")])));
        assert_eq!(s.rooms.len(), 2);
        assert!(s.loaded);
        assert!(s.begin_load()); // idle again
    }
}
