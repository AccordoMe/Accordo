// SPDX-License-Identifier: GPL-3.0-or-later
//! Keyboard shortcuts: every action, its default keys and the user's own. The key handler
//! (app/keys.rs) asks `action`; App settings → Shortcuts lists `GROUPS` and records new keys.
//!
//! The user's keys live in state.json (`AppStore::hotkeys`): action id → "Ctrl+Shift+K", "" for
//! none. Only what differs from the default is kept, so a new default reaches everyone who didn't
//! change that action. Ctrl is ⌘ on macOS (iced's "command" modifier).
//!
//! Keys are read in a way that doesn't depend on the layout: letters by what they type on a Latin
//! layout and by their place on others (Ctrl+K is Ctrl+Л on ЙЦУКЕН), digits and punctuation by
//! their place (Ctrl+, is Ctrl+Б), so Shift doesn't turn Ctrl+1 into Ctrl+!.

use std::collections::BTreeMap;
use std::fmt;

use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{Key, Modifiers};

use crate::app::Nav;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    QuickSwitcher,
    PrevChat,
    NextChat,
    PrevUnread,
    NextUnread,
    Page(Nav),
    ToggleRail,
    NewChat,
    NewGroup,
    JoinLink,
    BrowsePublic,
    ToggleMembers,
    Pinned,
    Search,
    RoomSettings,
    Emoji,
    Gifs,
    Attach,
    VoiceMessage,
    OpenWallet,
    Contacts,
    ChainSettings,
    NextChain,
    PrevChain,
    LockWallets,
    HideScreen,
    AccountMenu,
    AppSettings,
    Shortcuts,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    AdminPanel,
}

/// One action as the Shortcuts tab shows it. `id` is what state.json keeps — never rename one.
pub struct Def {
    pub action: Action,
    pub id: &'static str,
    pub label: &'static str,
    /// "" = no keys unless the user gives it some.
    pub default: &'static str,
}

pub struct Group {
    pub title: &'static str,
    pub items: &'static [Def],
}

const fn def(action: Action, id: &'static str, label: &'static str, default: &'static str) -> Def {
    Def { action, id, label, default }
}

pub const GROUPS: &[Group] = &[
    Group {
        title: "Navigation",
        items: &[
            def(Action::QuickSwitcher, "quick_switcher", "Jump to a chat", "Ctrl+K"),
            def(Action::PrevChat, "prev_chat", "Previous chat in the list", "Alt+Up"),
            def(Action::NextChat, "next_chat", "Next chat in the list", "Alt+Down"),
            def(Action::PrevUnread, "prev_unread", "Previous unread chat", "Alt+Shift+Up"),
            def(Action::NextUnread, "next_unread", "Next unread chat", "Alt+Shift+Down"),
            def(Action::Page(Nav::Chat), "page.chat", "Chats", "Ctrl+1"),
            def(Action::Page(Nav::Cheques), "page.cheques", "Cheques", "Ctrl+2"),
            def(Action::Page(Nav::Invoices), "page.invoices", "Invoices", "Ctrl+3"),
            def(Action::Page(Nav::Drops), "page.drops", "Token drops", "Ctrl+4"),
            def(Action::Page(Nav::Crowdfund), "page.crowdfund", "Crowdfunding", "Ctrl+5"),
            def(Action::Page(Nav::Settings), "page.settings", "Settings", "Ctrl+6"),
            def(Action::ToggleRail, "toggle_rail", "Show / hide the side rail", "Ctrl+B"),
        ],
    },
    Group {
        title: "Chat",
        items: &[
            def(Action::NewChat, "new_dm", "New direct message", "Ctrl+N"),
            def(Action::NewGroup, "new_group", "New group", "Ctrl+Shift+N"),
            def(Action::JoinLink, "join_link", "Join with an invite link", "Ctrl+Shift+J"),
            def(Action::BrowsePublic, "browse_public", "Browse public chats", ""),
            def(Action::ToggleMembers, "members", "Members list", "Ctrl+Shift+M"),
            def(Action::Pinned, "pinned", "Pinned messages", "Ctrl+Shift+P"),
            def(Action::Search, "search", "Search in this chat", "Ctrl+F"),
            def(Action::RoomSettings, "room_settings", "This chat's settings", "Ctrl+Shift+S"),
            def(Action::Emoji, "emoji", "Emoji", "Ctrl+E"),
            def(Action::Gifs, "gifs", "GIFs & stickers", "Ctrl+G"),
            def(Action::Attach, "attach", "Attach — the + menu", "Ctrl+U"),
            def(Action::VoiceMessage, "voice", "Voice message — again to send it", ""),
        ],
    },
    Group {
        title: "Wallets & networks",
        items: &[
            def(Action::OpenWallet, "wallets", "Wallets", "Ctrl+Shift+W"),
            def(Action::Contacts, "contacts", "Contacts — your address book", "Ctrl+Shift+T"),
            def(Action::ChainSettings, "chain_settings", "Network settings — RPC, explorer", "Ctrl+Shift+R"),
            def(Action::NextChain, "next_chain", "Next network", ""),
            def(Action::PrevChain, "prev_chain", "Previous network", ""),
            def(Action::LockWallets, "lock_wallets", "Lock wallets now", "Ctrl+Shift+L"),
        ],
    },
    Group {
        title: "Privacy & account",
        items: &[
            def(Action::HideScreen, "hide_screen", "Hide the window's content", "Ctrl+Shift+H"),
            def(Action::AccountMenu, "account_menu", "Account menu — switch accounts", ""),
        ],
    },
    Group {
        title: "App",
        items: &[
            def(Action::AppSettings, "app_settings", "App settings", "Ctrl+,"),
            def(Action::Shortcuts, "shortcuts", "Keyboard shortcuts — this list", "Ctrl+/"),
            def(Action::ZoomIn, "zoom_in", "Zoom in", "Ctrl+="),
            def(Action::ZoomOut, "zoom_out", "Zoom out", "Ctrl+-"),
            def(Action::ZoomReset, "zoom_reset", "Actual size", "Ctrl+0"),
            def(Action::AdminPanel, "admin", "Admin panel (admins)", ""),
        ],
    },
];

/// Keys that always work, shortcuts on or off, and can't be changed.
pub const ALWAYS: &[(&[&str], &str)] = &[
    (&["Esc"], "Close the menu, panel or reply"),
    (&["Tab"], "Next field (Shift+Tab: previous)"),
    (&["F12"], "Developer tools (Developer mode) — or Ctrl+Shift+I"),
];

/// The user's keys: action id → combination, "" = none (AppStore::hotkeys).
pub type Overrides = BTreeMap<String, String>;

/// A key with its modifiers, as state.json writes it: "Ctrl+Alt+Shift+K".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combo {
    /// Ctrl, or ⌘ on macOS.
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// "K", "1", ",", "Up", "PageDown", "F5"…
    pub key: String,
}

impl Combo {
    /// The combination a key press makes; None for a modifier alone or a key shortcuts don't use.
    pub fn from_event(key: &Key, physical: Physical, m: Modifiers) -> Option<Combo> {
        // Super / Windows is the desktop's (on macOS it's ⌘ — `command` below).
        if m.logo() && !cfg!(target_os = "macos") {
            return None;
        }
        Some(Combo { ctrl: m.command(), alt: m.alt(), shift: m.shift(), key: key_name(key, physical)? })
    }

    pub fn parse(s: &str) -> Option<Combo> {
        let mut parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let key = parts.pop().filter(|k| !k.is_empty())?;
        let mut c = Combo { ctrl: false, alt: false, shift: false, key: key.to_string() };
        for p in parts {
            match p.to_ascii_lowercase().as_str() {
                "ctrl" | "cmd" => c.ctrl = true,
                "alt" => c.alt = true,
                "shift" => c.shift = true,
                _ => return None,
            }
        }
        Some(c)
    }

    /// What the key caps show: ["Ctrl", "Shift", "↑"].
    pub fn chips(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for (on, name) in [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift")] {
            if on {
                v.push(name.into());
            }
        }
        v.push(
            match self.key.as_str() {
                "Up" => "↑",
                "Down" => "↓",
                "Left" => "←",
                "Right" => "→",
                k => k,
            }
            .into(),
        );
        v
    }

    fn function_key(&self) -> bool {
        self.key.len() > 1 && self.key.starts_with('F') && self.key[1..].chars().all(|c| c.is_ascii_digit())
    }
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [(self.ctrl, "Ctrl+"), (self.alt, "Alt+"), (self.shift, "Shift+")] {
            if on {
                f.write_str(name)?;
            }
        }
        f.write_str(&self.key)
    }
}

fn key_name(key: &Key, physical: Physical) -> Option<String> {
    if let Key::Named(n) = key {
        let name = match n {
            Named::ArrowUp => "Up",
            Named::ArrowDown => "Down",
            Named::ArrowLeft => "Left",
            Named::ArrowRight => "Right",
            Named::PageUp => "PageUp",
            Named::PageDown => "PageDown",
            Named::Home => "Home",
            Named::End => "End",
            Named::Enter => "Enter",
            Named::Space => "Space",
            Named::Backspace => "Backspace",
            Named::Delete => "Delete",
            Named::Insert => "Insert",
            Named::F1 => "F1",
            Named::F2 => "F2",
            Named::F3 => "F3",
            Named::F4 => "F4",
            Named::F5 => "F5",
            Named::F6 => "F6",
            Named::F7 => "F7",
            Named::F8 => "F8",
            Named::F9 => "F9",
            Named::F10 => "F10",
            Named::F11 => "F11",
            Named::F12 => "F12",
            _ => return None,
        };
        return Some(name.into());
    }
    // Digits and punctuation by their place: Shift or the layout doesn't change them.
    if let Physical::Code(code) = physical {
        let by_place = match code {
            Code::Digit0 => Some('0'),
            Code::Digit1 => Some('1'),
            Code::Digit2 => Some('2'),
            Code::Digit3 => Some('3'),
            Code::Digit4 => Some('4'),
            Code::Digit5 => Some('5'),
            Code::Digit6 => Some('6'),
            Code::Digit7 => Some('7'),
            Code::Digit8 => Some('8'),
            Code::Digit9 => Some('9'),
            Code::Minus => Some('-'),
            Code::Equal => Some('='),
            Code::BracketLeft => Some('['),
            Code::BracketRight => Some(']'),
            Code::Backslash => Some('\\'),
            Code::Semicolon => Some(';'),
            Code::Quote => Some('\''),
            Code::Comma => Some(','),
            Code::Period => Some('.'),
            Code::Slash => Some('/'),
            Code::Backquote => Some('`'),
            _ => None,
        };
        if let Some(c) = by_place {
            return Some(c.to_string());
        }
    }
    let c = key.to_latin(physical)?;
    c.is_ascii_alphabetic().then(|| c.to_ascii_uppercase().to_string())
}

/// Why these keys can't be a shortcut (None: they can).
pub fn refuse(c: &Combo) -> Option<String> {
    let k = c.key.as_str();
    if !(c.ctrl || c.alt || c.function_key()) {
        return Some("Add Ctrl or Alt — keys on their own are for typing".into());
    }
    if k == "F12" || (c.ctrl && c.shift && !c.alt && k == "I") {
        return Some("F12 and Ctrl+Shift+I open the developer tools".into());
    }
    if c.alt && !c.ctrl && k == "F4" {
        return Some("Alt+F4 closes the window".into());
    }
    if c.ctrl && !c.alt && ["A", "C", "V", "X", "Z", "Y"].contains(&k) {
        return Some(format!("{c} is for text: select all, copy, paste, cut, undo"));
    }
    if c.ctrl && ["Left", "Right", "Backspace", "Delete", "Home", "End"].contains(&k) {
        return Some(format!("{c} moves or deletes by word in text fields"));
    }
    None
}

pub fn defs() -> impl Iterator<Item = &'static Def> {
    GROUPS.iter().flat_map(|g| g.items.iter())
}

pub fn def_of(action: Action) -> &'static Def {
    defs().find(|d| d.action == action).expect("every action is in GROUPS")
}

/// This action's keys now (the user's, else the default).
pub fn binding(over: &Overrides, d: &Def) -> Option<Combo> {
    Combo::parse(over.get(d.id).map(String::as_str).unwrap_or(d.default))
}

pub fn binding_of(over: &Overrides, action: Action) -> Option<Combo> {
    binding(over, def_of(action))
}

/// The user changed this one (keys of their own, or none).
pub fn changed(over: &Overrides, d: &Def) -> bool {
    binding(over, d) != Combo::parse(d.default)
}

fn set(over: &mut Overrides, d: &Def, combo: Option<&Combo>) {
    if combo.cloned() == Combo::parse(d.default) {
        over.remove(d.id);
    } else {
        over.insert(d.id.into(), combo.map(Combo::to_string).unwrap_or_default());
    }
}

/// Gives `action` these keys; an action that had them loses them — its label is returned.
pub fn assign(over: &mut Overrides, action: Action, combo: &Combo) -> Option<&'static str> {
    let mut took = None;
    for d in defs() {
        if d.action != action && binding(over, d).as_ref() == Some(combo) {
            set(over, d, None);
            took = Some(d.label);
        }
    }
    set(over, def_of(action), Some(combo));
    took
}

/// No keys for this action.
pub fn clear(over: &mut Overrides, action: Action) {
    set(over, def_of(action), None);
}

/// Back to the default keys (taken from whatever had them meanwhile — its label is returned).
pub fn reset(over: &mut Overrides, action: Action) -> Option<&'static str> {
    let d = def_of(action);
    match Combo::parse(d.default) {
        Some(c) => assign(over, action, &c),
        None => {
            over.remove(d.id);
            None
        }
    }
}

/// The developer tools' keys: F12, or Ctrl+Shift+I as in browsers (laptops often need Fn for F12).
/// By the key's place, so it works in any layout (Ш on a Russian one).
pub fn devtools(key: &Key, physical: Physical, m: Modifiers) -> bool {
    matches!(key, Key::Named(Named::F12)) || (m.control() && m.shift() && !m.alt() && key.to_latin(physical).is_some_and(|c| c.eq_ignore_ascii_case(&'i')))
}

/// The action of a key press (plain typing keys never are one: see `refuse`).
pub fn action(over: &Overrides, key: &Key, physical: Physical, m: Modifiers) -> Option<Action> {
    let combo = Combo::from_event(key, physical, m)?;
    if refuse(&combo).is_some() {
        return None;
    }
    let find = |c: &Combo| defs().find(|d| binding(over, d).as_ref() == Some(c)).map(|d| d.action);
    find(&combo).or_else(|| {
        // Ctrl++ needs Shift on most layouts (and Ctrl+_ sits next to it): with or without it.
        (combo.shift && (combo.key == "=" || combo.key == "-")).then(|| Combo { shift: false, ..combo.clone() }).and_then(|c| find(&c))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(c: &str, code: Code, m: Modifiers) -> Option<Action> {
        action(&Overrides::new(), &Key::Character(c.into()), Physical::Code(code), m)
    }

    #[test]
    fn shortcuts_match_in_any_layout() {
        assert_eq!(press("k", Code::KeyK, Modifiers::CTRL), Some(Action::QuickSwitcher));
        // Russian layout: the same key gives "л"; "," is "б" there.
        assert_eq!(press("л", Code::KeyK, Modifiers::CTRL), Some(Action::QuickSwitcher));
        assert_eq!(press("б", Code::Comma, Modifiers::CTRL), Some(Action::AppSettings));
        assert_eq!(press("H", Code::KeyH, Modifiers::CTRL | Modifiers::SHIFT), Some(Action::HideScreen));
        assert_eq!(press("3", Code::Digit3, Modifiers::CTRL), Some(Action::Page(Nav::Invoices)));
        assert_eq!(press("k", Code::KeyK, Modifiers::empty()), None);
        assert_eq!(action(&Overrides::new(), &Key::Named(Named::ArrowDown), Physical::Code(Code::ArrowDown), Modifiers::ALT), Some(Action::NextChat));
        assert_eq!(action(&Overrides::new(), &Key::Named(Named::ArrowDown), Physical::Code(Code::ArrowDown), Modifiers::ALT | Modifiers::SHIFT), Some(Action::NextUnread));
    }

    #[test]
    fn zoom_works_with_or_without_shift() {
        assert_eq!(press("=", Code::Equal, Modifiers::CTRL), Some(Action::ZoomIn));
        assert_eq!(press("+", Code::Equal, Modifiers::CTRL | Modifiers::SHIFT), Some(Action::ZoomIn));
        assert_eq!(press("_", Code::Minus, Modifiers::CTRL | Modifiers::SHIFT), Some(Action::ZoomOut));
    }

    #[test]
    fn defaults_are_valid_and_distinct() {
        let mut seen: Vec<Combo> = Vec::new();
        let mut ids: Vec<&str> = Vec::new();
        for d in defs() {
            assert!(!ids.contains(&d.id), "{} twice", d.id);
            ids.push(d.id);
            if d.default.is_empty() {
                continue;
            }
            let c = Combo::parse(d.default).unwrap_or_else(|| panic!("{}: can't read {}", d.id, d.default));
            assert_eq!(c.to_string(), d.default, "{} isn't written the way state.json writes it", d.id);
            assert_eq!(refuse(&c), None, "{}", d.id);
            assert!(!seen.contains(&c), "{} is used twice", d.default);
            seen.push(c);
        }
    }

    #[test]
    fn new_keys_take_over_and_reset_brings_them_back() {
        let mut over = Overrides::new();
        let ctrl_k = Combo::parse("Ctrl+K").unwrap();
        assert_eq!(assign(&mut over, Action::NewChat, &ctrl_k), Some("Jump to a chat"));
        assert_eq!(action(&over, &Key::Character("k".into()), Physical::Code(Code::KeyK), Modifiers::CTRL), Some(Action::NewChat));
        assert_eq!(binding_of(&over, Action::QuickSwitcher), None);
        assert_eq!(over.get("quick_switcher").map(String::as_str), Some(""));
        assert_eq!(over.get("new_dm").map(String::as_str), Some("Ctrl+K"));

        // Its default back: Ctrl+K leaves "New direct message" again.
        assert_eq!(reset(&mut over, Action::QuickSwitcher), Some("New direct message"));
        assert_eq!(binding_of(&over, Action::QuickSwitcher), Some(ctrl_k));
        assert_eq!(binding_of(&over, Action::NewChat), None);
        reset(&mut over, Action::NewChat);
        assert!(over.is_empty(), "defaults aren't stored: {over:?}");

        clear(&mut over, Action::Emoji);
        assert_eq!(press("e", Code::KeyE, Modifiers::CTRL), Some(Action::Emoji));
        assert_eq!(action(&over, &Key::Character("e".into()), Physical::Code(Code::KeyE), Modifiers::CTRL), None);
    }

    #[test]
    fn text_keys_are_refused() {
        for s in ["K", "Shift+K", "Ctrl+C", "Ctrl+Shift+V", "Ctrl+Backspace", "F12", "Ctrl+Shift+I", "Alt+F4"] {
            assert!(refuse(&Combo::parse(s).unwrap()).is_some(), "{s}");
        }
        for s in ["F5", "Alt+K", "Ctrl+Alt+C", "Ctrl+Shift+W"] {
            assert_eq!(refuse(&Combo::parse(s).unwrap()), None, "{s}");
        }
    }
}
