// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's optional sections — cheques, invoices, drops, crowdfunding — and whether each is on.
//!
//! Three things decide it:
//! - the build: a module compiled out isn't there at all;
//! - the server: a section it closes for everyone (the web's closedSections.ts);
//! - the user: App settings → Modules hides a section they don't use.
//!
//! The two kinds of "off" differ on purpose. A module the user hides leaves the menus (the rail,
//! the chat's + menu, the attach window, Ctrl+1…6, the start page), but money sent to them still
//! shows in the chat and can be claimed or paid: hiding a section must never hide someone's
//! cheque. A module the build or the server doesn't have is closed everywhere.

use serde::{Deserialize, Serialize};

use crate::app::Nav;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Module {
    Cheques,
    Invoices,
    Drops,
    Crowdfund,
}

pub const ALL: [Module; 4] = [Module::Cheques, Module::Invoices, Module::Drops, Module::Crowdfund];

impl Module {
    pub fn label(self) -> &'static str {
        match self {
            Module::Cheques => "Cheques",
            Module::Invoices => "Invoices",
            Module::Drops => "Token drops",
            Module::Crowdfund => "Crowdfunding",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Module::Cheques => "Lock crypto for someone; they claim it when the work is done.",
            Module::Invoices => "Bill a client; they pay on-chain with the PIN.",
            Module::Drops => "Send a token to a list of wallets, at once or in parts over time.",
            Module::Crowdfund => "Raise money for a goal; backers get it back if it isn't reached.",
        }
    }

    pub fn nav(self) -> Nav {
        match self {
            Module::Cheques => Nav::Cheques,
            Module::Invoices => Nav::Invoices,
            Module::Drops => Nav::Drops,
            Module::Crowdfund => Nav::Crowdfund,
        }
    }

    /// The module a page belongs to (None: chat and settings, always there).
    pub fn of_nav(nav: Nav) -> Option<Module> {
        ALL.into_iter().find(|m| m.nav() == nav)
    }

    /// The id App settings uses for the start page.
    pub fn page_id(self) -> &'static str {
        match self {
            Module::Cheques => "cheques",
            Module::Invoices => "invoices",
            Module::Drops => "drops",
            Module::Crowdfund => "crowdfund",
        }
    }

    /// Whether this build has the module (cargo features `drops`, `crowdfund`; cheques and
    /// invoices are woven through the chat and always built).
    pub fn compiled(self) -> bool {
        match self {
            Module::Cheques | Module::Invoices => true,
            Module::Drops => cfg!(feature = "drops"),
            Module::Crowdfund => cfg!(feature = "crowdfund"),
        }
    }

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// Why a module is off (None from `Modules::off_reason`: it's on).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Off {
    /// Not in this build.
    Build,
    /// Closed by the server for everyone.
    Server,
    /// Hidden by the user.
    User,
}

/// Which modules are on, worked out once from the three sources. Small and Copy: views get it by
/// value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modules {
    /// In the build and open on the server: its cards work in the chat.
    available: u8,
    /// Available and not hidden by the user: in the menus.
    shown: u8,
    server_closed: u8,
}

impl Default for Modules {
    fn default() -> Self {
        Self::new(&[], &[])
    }
}

impl Modules {
    pub fn new(server_closed: &[Module], user_hidden: &[Module]) -> Self {
        let mut m = Modules { available: 0, shown: 0, server_closed: 0 };
        for x in ALL {
            let closed = server_closed.contains(&x);
            if closed {
                m.server_closed |= x.bit();
            }
            if x.compiled() && !closed {
                m.available |= x.bit();
                if !user_hidden.contains(&x) {
                    m.shown |= x.bit();
                }
            }
        }
        m
    }

    /// In the menus: the user can open the page and start new ones.
    pub fn shown(self, m: Module) -> bool {
        self.shown & m.bit() != 0
    }

    /// Usable at all: cards in the chat, claiming and paying what was sent to you.
    pub fn available(self, m: Module) -> bool {
        self.available & m.bit() != 0
    }

    pub fn off_reason(self, m: Module) -> Option<Off> {
        if !m.compiled() {
            Some(Off::Build)
        } else if self.server_closed & m.bit() != 0 {
            Some(Off::Server)
        } else if !self.shown(m) {
            Some(Off::User)
        } else {
            None
        }
    }

    /// Whether a page can be opened from a menu, a shortcut or the start page.
    pub fn page_shown(self, nav: Nav) -> bool {
        Module::of_nav(nav).is_none_or(|m| self.shown(m))
    }

    /// Whether a page can be opened at all (a chat card's "Back this campaign" opens a page the
    /// user hid from the menu: they're acting on something sent to them).
    pub fn page_available(self, nav: Nav) -> bool {
        Module::of_nav(nav).is_none_or(|m| self.available(m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cheques and invoices are in every build: the rules are checked on them.

    #[test]
    fn everything_built_is_on_by_default() {
        let m = Modules::default();
        for x in ALL.into_iter().filter(|x| x.compiled()) {
            assert!(m.shown(x) && m.available(x));
            assert_eq!(m.off_reason(x), None);
        }
        assert!(m.page_shown(Nav::Chat) && m.page_shown(Nav::Settings));
    }

    #[test]
    fn hidden_by_the_user_still_works_in_chat() {
        let m = Modules::new(&[], &[Module::Invoices]);
        assert!(!m.shown(Module::Invoices));
        assert!(m.available(Module::Invoices), "an invoice sent to you can still be paid");
        assert_eq!(m.off_reason(Module::Invoices), Some(Off::User));
        assert!(!m.page_shown(Nav::Invoices) && m.page_available(Nav::Invoices));
        assert!(m.shown(Module::Cheques));
    }

    #[test]
    fn closed_by_the_server_is_closed_everywhere() {
        let m = Modules::new(&[Module::Cheques], &[Module::Cheques]);
        assert!(!m.shown(Module::Cheques) && !m.available(Module::Cheques));
        assert_eq!(m.off_reason(Module::Cheques), Some(Off::Server), "the server's reason wins over the user's");
        assert!(!m.page_available(Nav::Cheques));
    }

    #[test]
    fn not_built_is_closed_whatever_else_says() {
        for x in ALL.into_iter().filter(|x| !x.compiled()) {
            let m = Modules::new(&[x], &[x]);
            assert!(!m.shown(x) && !m.available(x));
            assert_eq!(m.off_reason(x), Some(Off::Build), "{x:?}: the build's reason comes first");
            assert!(!m.page_available(x.nav()));
        }
        assert_eq!(Module::Drops.compiled(), cfg!(feature = "drops"));
        assert_eq!(Module::Crowdfund.compiled(), cfg!(feature = "crowdfund"));
    }

    #[test]
    fn saved_names_are_stable() {
        assert_eq!(serde_json::to_string(&Module::Crowdfund).unwrap(), "\"crowdfund\"");
        let back: Vec<Module> = serde_json::from_str("[\"drops\",\"invoices\"]").unwrap();
        assert_eq!(back, vec![Module::Drops, Module::Invoices]);
    }
}
