// SPDX-License-Identifier: GPL-3.0-or-later
//! Crowdfunding (CrowdfundPage.tsx): launch a campaign, back one, the protocol in short — and a
//! campaign's own window (CampaignDetailsCard.tsx): its story and numbers, its backers and what they
//! wrote, what you can do with it, and its public page (/fund/<id>) to share.

use iced::widget::{button, checkbox, column, container, mouse_area, responsive, rich_text, row, scrollable, space, span, stack, svg, text, Column, Row};
use iced::{Alignment, Color, Fill, Font, Length, Padding};

use super::Effect;
use crate::api::client::Api;
use crate::api::crowdfund::{CampaignContributionRead, CampaignDetail, CampaignRead, CampaignStatus, MyCampaigns};
use crate::api::data::Token;
use crate::flows::crowdfund::{Action, BackDraft, LaunchDraft};
use crate::app::Message;
use crate::components::finance::duration_picker::{self, Duration};
use crate::components::finance::{fee_picker, token_select};
use crate::components::form::{input, label};
use crate::components::page_tabs::{self, Tab};
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::{self, *};

const DURATIONS: [Duration; 4] = [Duration::Days(7), Duration::Days(14), Duration::Days(30), Duration::Days(60)];
const CATEGORIES: [&str; 6] = ["Tech & Dev", "Charity", "Gaming", "DeFi", "Creative", "Community"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrowdfundTab {
    /// Back a campaign: the form, and the campaign itself as a card.
    Back,
    /// Launch one: the form, and a live preview of its card.
    Launch,
    Explore,
    Mine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    Flexible,
    AllOrNothing,
}

/// The campaign window's sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Story,
    Backers,
}

/// A campaign opened in its window.
pub struct Opened {
    pub campaign: CampaignRead,
    pub tab: DetailTab,
    /// Its contributions, newest first (None: loading).
    pub backers: Option<Result<Vec<CampaignContributionRead>, String>>,
}

#[derive(Debug, Clone)]
pub enum CrowdfundMsg {
    Tab(CrowdfundTab),
    /// Open a campaign in its window (then read its details and backers).
    Open(Box<CampaignRead>),
    Close,
    Detail(DetailTab),
    Detailed(String, Result<Box<CampaignDetail>, String>),
    /// Its public page (/fund/<id>): copy the link / open it in the browser.
    CopyLink(String),
    OpenPage(String),
    Title(String),
    Category(usize),
    Target(String),
    Token(Token),
    Model(Model),
    Duration(Duration),
    Fee(u32),
    Description(String),
    /// The creator accepts the Crowdfunding Terms.
    Agree(bool),
    Launch,
    CampaignId(String),
    Contribution(String),
    /// 25 / 50 / 75 / 100 (MAX) % of what the wallet can spend.
    Share(u8),
    /// That share, read for this campaign: the amount to fill in.
    Shared(String, Result<String, String>),
    BackerNote(String),
    Understood(bool),
    Terms,
    Back,
    /// The campaign typed in, as Accordo lists it.
    Looked(String, Result<Box<CampaignRead>, String>),
    Explore(Result<Vec<CampaignRead>, String>),
    Mine(Result<Box<MyCampaigns>, String>),
    Refresh,
    /// Back this one: its id into the form.
    Pick(String),
    Act(Box<CampaignRead>, Action),
}

pub struct State {
    pub tab: CrowdfundTab,
    pub title: String,
    pub category: usize,
    pub target: String,
    pub token: Option<Token>,
    pub model: Model,
    pub duration: Duration,
    pub fee_bps: u32,
    pub campaign_id: String,
    pub contribution: String,
    pub backer_note: String,
    pub understood: bool,
    pub description: String,
    pub agreed: bool,
    /// The campaign the id names (None: not looked up / not a full id yet).
    pub looked: Option<Result<CampaignRead, String>>,
    pub explore: Option<Result<Vec<CampaignRead>, String>>,
    pub mine: Option<Result<MyCampaigns, String>>,
    /// The campaign window, when one is open.
    pub open: Option<Opened>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tab: CrowdfundTab::Back,
            title: String::new(),
            category: 0,
            target: String::new(),
            token: None,
            model: Model::Flexible,
            duration: Duration::Days(30),
            fee_bps: fee_picker::DEFAULT_BPS,
            campaign_id: String::new(),
            contribution: String::new(),
            backer_note: String::new(),
            understood: false,
            description: String::new(),
            agreed: false,
            looked: None,
            explore: None,
            mine: None,
            open: None,
        }
    }
}

/// The lists (Explore / My Pools) again.
pub fn reload(state: &mut State, api: &Api) -> iced::Task<Message> {
    let mut tasks = vec![load_tab(state, api, true)];
    if let Some(Ok(c)) = &state.looked {
        let id = c.campaign_id.clone();
        tasks.push(lookup(api, id));
    }
    // The open window shows the change too (a contribution, a withdrawal, refunds).
    if let Some(o) = &state.open {
        tasks.push(detail(api, o.campaign.campaign_id.clone()));
    }
    iced::Task::batch(tasks)
}

/// A campaign's details and backers, for its window.
fn detail(api: &Api, id: String) -> iced::Task<Message> {
    let k = id.clone();
    iced::Task::perform(crate::api::crowdfund::get(api.clone(), id), move |r| msg(CrowdfundMsg::Detailed(k.clone(), r.map(Box::new).map_err(|e| e.message))))
}

fn lookup(api: &Api, id: String) -> iced::Task<Message> {
    let k = id.clone();
    iced::Task::perform(crate::flows::crowdfund::lookup(api.clone(), id), move |r| msg(CrowdfundMsg::Looked(k.clone(), r.map(Box::new))))
}

/// The open tab's list, if it isn't loaded (or `again`).
pub fn load_tab(state: &mut State, api: &Api, again: bool) -> iced::Task<Message> {
    match state.tab {
        CrowdfundTab::Explore if again || state.explore.is_none() => iced::Task::perform(crate::api::crowdfund::active(api.clone()), |r| msg(CrowdfundMsg::Explore(r.map_err(|e| e.message)))),
        CrowdfundTab::Mine if again || state.mine.is_none() => iced::Task::perform(crate::api::crowdfund::get_my(api.clone()), |r| msg(CrowdfundMsg::Mine(r.map(Box::new).map_err(|e| e.message)))),
        _ => iced::Task::none(),
    }
}

/// After launching: the next campaign starts clean.
pub fn launched(state: &mut State) {
    state.title.clear();
    state.description.clear();
    state.target.clear();
    state.agreed = false;
    state.mine = None;
}

/// After backing: the amount and note clear.
pub fn backed(state: &mut State) {
    state.contribution.clear();
    state.backer_note.clear();
}

/// CreateCampaignCard's checks, in its order and with its words.
fn launch_draft(state: &State, now: i64) -> Result<LaunchDraft, String> {
    if !state.agreed {
        return Err("Please read and accept the Crowdfunding Terms first".into());
    }
    if state.title.trim().is_empty() {
        return Err("Please enter a campaign title!".into());
    }
    let Some(token) = state.token.clone() else { return Err("Choose a token first".into()) };
    let units = crate::evm::client::parse_units(&state.target, token.decimals).ok().filter(|u| !u.is_zero()).ok_or("Please enter a valid funding target amount!")?;
    let target_units: u128 = units.try_into().map_err(|_| "That target is too large".to_string())?;
    let deadline = state.duration.ends_at(now).ok_or("Pick when the campaign ends")?;
    if deadline - now < 3_600 + 60 {
        return Err("The deadline must be at least an hour from now.".into());
    }
    if deadline - now > 365 * 86_400 {
        return Err("The deadline can be at most a year from now.".into());
    }
    Ok(LaunchDraft {
        title: state.title.trim().to_string(),
        description: state.description.trim().to_string(),
        category: CATEGORIES[state.category.min(CATEGORIES.len() - 1)].to_string(),
        token,
        target: state.target.trim().to_string(),
        target_units,
        deadline: deadline as u64,
        fee_bps: state.fee_bps.min(u32::from(u16::MAX)) as u16,
        all_or_nothing: state.model == Model::AllOrNothing,
    })
}

pub fn update(state: &mut State, msg: CrowdfundMsg, api: Option<Api>) -> Effect {
    match msg {
        CrowdfundMsg::Tab(tab) => {
            state.tab = tab;
            return api.map(|a| Effect::Run(load_tab(state, &a, false))).unwrap_or(Effect::None);
        }
        CrowdfundMsg::Description(v) => state.description = v,
        CrowdfundMsg::Agree(v) => state.agreed = v,
        CrowdfundMsg::Looked(id, r) => {
            if id.trim().eq_ignore_ascii_case(state.campaign_id.trim()) {
                state.looked = Some(r.map(|b| *b));
            }
        }
        CrowdfundMsg::Explore(r) => state.explore = Some(r),
        CrowdfundMsg::Mine(r) => state.mine = Some(r.map(|b| *b)),
        CrowdfundMsg::Refresh => return api.map(|a| Effect::Run(load_tab(state, &a, true))).unwrap_or(Effect::None),
        CrowdfundMsg::Open(c) => {
            let id = c.campaign_id.clone();
            state.open = Some(Opened { campaign: *c, tab: DetailTab::Story, backers: None });
            return api.map(|a| Effect::Run(detail(&a, id))).unwrap_or(Effect::None);
        }
        CrowdfundMsg::Close => state.open = None,
        CrowdfundMsg::Detail(t) => {
            if let Some(o) = state.open.as_mut() {
                o.tab = t;
            }
        }
        CrowdfundMsg::Detailed(id, r) => {
            if let Some(o) = state.open.as_mut().filter(|o| o.campaign.campaign_id.eq_ignore_ascii_case(&id)) {
                match r {
                    Ok(d) => {
                        let d = *d;
                        o.campaign = d.campaign;
                        o.backers = Some(Ok(d.contributions));
                    }
                    Err(e) => o.backers = Some(Err(e)),
                }
            }
        }
        CrowdfundMsg::CopyLink(id) => return Effect::CopyWeb(format!("/fund/{id}"), "Campaign link"),
        CrowdfundMsg::OpenPage(id) => return Effect::OpenWeb(format!("/fund/{id}")),
        CrowdfundMsg::Pick(id) => {
            state.open = None;
            state.tab = CrowdfundTab::Back;
            state.campaign_id = id.clone();
            state.looked = None;
            return api.map(|a| Effect::Run(lookup(&a, id))).unwrap_or(Effect::None);
        }
        CrowdfundMsg::Act(c, a) => return Effect::CampaignAction(c, a),
        CrowdfundMsg::Title(v) => state.title = v,
        CrowdfundMsg::Category(i) => state.category = i,
        CrowdfundMsg::Target(v) => state.target = v,
        CrowdfundMsg::Token(t) => state.token = Some(t),
        CrowdfundMsg::Model(m) => state.model = m,
        CrowdfundMsg::Duration(Duration::Custom) => return Effect::PickTime(crate::components::time_picker::Purpose::CampaignEnd, match state.duration { Duration::At(t) => Some(t), _ => None }),
        CrowdfundMsg::Duration(d) => state.duration = d,
        CrowdfundMsg::Fee(b) => state.fee_bps = b,
        CrowdfundMsg::Launch => {
            return match launch_draft(state, chrono::Utc::now().timestamp()) {
                Ok(d) => Effect::LaunchCampaign(Box::new(d)),
                Err(e) => Effect::Error(e),
            }
        }
        CrowdfundMsg::CampaignId(v) => {
            state.campaign_id = v.trim().to_string();
            state.looked = None;
            if state.campaign_id.starts_with("0x") && state.campaign_id.len() == 66 {
                return api.map(|a| Effect::Run(lookup(&a, state.campaign_id.clone()))).unwrap_or(Effect::None);
            }
        }
        CrowdfundMsg::Contribution(v) => state.contribution = v,
        CrowdfundMsg::Share(pct) => return Effect::ContributionShare(pct),
        CrowdfundMsg::Shared(id, r) => {
            if !id.eq_ignore_ascii_case(state.campaign_id.trim()) {
                return Effect::None; // another campaign was picked meanwhile
            }
            match r {
                Ok(amount) => state.contribution = amount,
                Err(e) => return Effect::Error(e),
            }
        }
        CrowdfundMsg::BackerNote(v) => state.backer_note = v,
        CrowdfundMsg::Understood(v) => state.understood = v,
        CrowdfundMsg::Terms => return Effect::OpenWeb("/terms/crowdfunding".into()),
        CrowdfundMsg::Back => {
            if state.campaign_id.trim().is_empty() {
                return Effect::Error("Please select a campaign to support!".into());
            }
            if !state.understood {
                return Effect::Error("Please accept the Crowdfunding Terms first".into());
            }
            return Effect::BackCampaign(Box::new(BackDraft { campaign_id: state.campaign_id.clone(), amount: state.contribution.clone(), message: state.backer_note.clone() }));
        }
    }
    Effect::None
}

fn msg(m: CrowdfundMsg) -> Message {
    Message::Crowdfund(m)
}

pub fn view<'a>(state: &'a State, tokens: token_select::Field, native: &'a str, me: &'a str) -> El<'a> {
    responsive(move |size| {
        let tabs = page_tabs::view(vec![
            Tab { icon: icons::coins(), label: "Back", count: None, active: state.tab == CrowdfundTab::Back, on_press: msg(CrowdfundMsg::Tab(CrowdfundTab::Back)) },
            Tab { icon: icons::flame(), label: "Launch", count: None, active: state.tab == CrowdfundTab::Launch, on_press: msg(CrowdfundMsg::Tab(CrowdfundTab::Launch)) },
            Tab { icon: icons::globe(), label: "Explore Pools", count: None, active: state.tab == CrowdfundTab::Explore, on_press: msg(CrowdfundMsg::Tab(CrowdfundTab::Explore)) },
            Tab { icon: icons::briefcase(), label: "My Pools", count: None, active: state.tab == CrowdfundTab::Mine, on_press: msg(CrowdfundMsg::Tab(CrowdfundTab::Mine)) },
        ]);
        let body: El = match state.tab {
            CrowdfundTab::Explore => explore_view(state, me),
            CrowdfundTab::Mine => mine_view(state),
            CrowdfundTab::Back => {
                let side: El = match state.looked.as_ref() {
                    Some(Ok(c)) => spotlight(Spot::of(c)),
                    _ => pick_hint(),
                };
                split(size.width, back_card(state, native), column![side, protocol_card()].spacing(20).into())
            }
            CrowdfundTab::Launch => {
                let preview = spotlight(Spot::draft(state, native));
                split(size.width, launch_card(state, tokens.clone(), native), column![preview, protocol_card()].spacing(20).into())
            }
        };
        let page: El = scrollable(container(column![tabs, body].spacing(24)).padding([20, 24]).width(Fill)).height(Fill).style(thin_scroll).into();
        match &state.open {
            Some(o) => stack![page, crate::components::overlay::modal(campaign_window(o, me), msg(CrowdfundMsg::Close))].into(),
            None => page,
        }
    })
    .into()
}

/// Form on the left, its campaign on the right; one above the other when narrow.
fn split<'a>(width: f32, form: El<'a>, side: El<'a>) -> El<'a> {
    if width >= 1050.0 {
        row![container(form).width(iced::FillPortion(6)), container(side).width(iced::FillPortion(5))].spacing(20).into()
    } else {
        column![form, side].spacing(20).into()
    }
}

/// What the big campaign card shows: a campaign from Accordo, or the one being launched.
struct Spot {
    title: String,
    creator: String,
    category: String,
    chain: String,
    model: &'static str,
    status: (&'static str, Color),
    raised: f64,
    target: f64,
    symbol: String,
    backers: i64,
    deadline: String,
    fee: String,
    description: String,
    preview: bool,
}

impl Spot {
    fn of(c: &CampaignRead) -> Self {
        let fee = c.fee_bps as f64 / 100.0;
        Spot {
            title: c.title.clone(),
            creator: c.creator_username.clone(),
            category: c.category.clone(),
            chain: c.chain_name.clone(),
            model: if c.is_all_or_nothing == Some(true) { "All-or-nothing" } else { "Flexible" },
            status: status_of(c),
            raised: c.raised_amount,
            target: c.target_amount,
            symbol: c.token_symbol.clone(),
            backers: c.contributors_count,
            deadline: deadline_text(c),
            fee: if c.fee_on_withdrawal == Some(false) { format!("{fee}% on each contribution") } else { format!("{fee}% when the creator withdraws") },
            description: c.description.clone().unwrap_or_default(),
            preview: false,
        }
    }

    /// The launch form as it stands.
    fn draft(state: &State, native: &str) -> Self {
        let now = chrono::Utc::now().timestamp();
        let deadline = state.duration.ends_at(now).and_then(|t| chrono::DateTime::from_timestamp(t, 0)).map(|t| format!("ends {}", t.with_timezone(&chrono::Local).format("%b %-d, %Y"))).unwrap_or_default();
        Spot {
            title: if state.title.trim().is_empty() { "Your campaign title".into() } else { state.title.trim().to_string() },
            creator: String::new(),
            category: CATEGORIES[state.category.min(CATEGORIES.len() - 1)].to_string(),
            chain: String::new(),
            model: if state.model == Model::AllOrNothing { "All-or-nothing" } else { "Flexible" },
            status: ("PREVIEW", INDIGO_SOFT),
            raised: 0.0,
            target: state.target.trim().replace(',', ".").parse().unwrap_or(0.0),
            symbol: state.token.as_ref().map(|t| t.symbol.clone()).unwrap_or_else(|| native.to_string()),
            backers: 0,
            deadline,
            fee: format!("{}% when you withdraw", state.fee_bps as f64 / 100.0),
            description: if state.description.trim().is_empty() { "What you're raising for, in a few lines.".into() } else { state.description.trim().to_string() },
            preview: true,
        }
    }
}

/// The category's colour for the cover tile.
fn cover_tint(category: &str) -> u32 {
    match category {
        "Tech & Dev" => 0x6366f1,
        "Charity" => 0xf43f5e,
        "Gaming" => 0x8b5cf6,
        "DeFi" => 0x10b981,
        "Creative" => 0xf59e0b,
        "Community" => 0x0ea5e9,
        _ => 0x64748b,
    }
}

fn fmt_amount(v: f64) -> String {
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The big campaign card: cover, title, who, chips, the raise, the numbers, the story.
fn spotlight<'a>(s: Spot) -> El<'a> {
    let tint = cover_tint(&s.category);
    let pct = if s.target > 0.0 { (s.raised / s.target).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let chip = |t: String, c: Color| -> El<'a> { container(text(t).size(11).color(c)).padding([3, 8]).style(boxed(rgba(0xffffff, 0.05), rgba(0xffffff, 0.06), 6.0)).into() };
    let cover = container(
        row![
            container(icon(icons::flame(), 26, Color::WHITE)).width(56).height(56).center(56).style(boxed(rgba(tint, 0.55), rgba(0xffffff, 0.15), 14.0)),
            space().width(Fill),
            container(text(s.status.0).size(10.5).font(BOLD).color(s.status.1)).padding([4, 10]).style(boxed(rgba(0x000000, 0.35), rgba(0xffffff, 0.08), 999.0)),
        ]
        .align_y(Alignment::Start),
    )
    .width(Fill)
    .height(110)
    .padding(16)
    .style(move |_| container::Style {
        background: Some(iced::Background::Gradient(iced::Gradient::Linear(iced::gradient::Linear::new(iced::Radians(2.4)).add_stop(0.0, rgba(tint, 0.45)).add_stop(1.0, rgba(tint, 0.06))))),
        border: border(rgba(tint, 0.3), 1.0, 14.0),
        ..Default::default()
    });
    let mut who = row![].spacing(6).align_y(Alignment::Center);
    if !s.creator.is_empty() {
        who = who.push(text(format!("by @{}", s.creator)).size(12.5).color(TEXT_SECONDARY));
    }
    who = who.push(chip(s.category.clone(), TEXT_SECONDARY)).push(chip(s.model.to_string(), INDIGO_SOFT));
    if !s.chain.is_empty() {
        who = who.push(chip(s.chain.clone(), TEXT_SECONDARY));
    }
    let raise = column![
        row![
            text(fmt_amount(s.raised)).size(26).font(BOLD).color(Color::WHITE),
            text(format!("{} raised", s.symbol)).size(13).color(TEXT_SECONDARY),
            space().width(Fill),
            text(format!("{}%", (pct * 100.0).round())).size(16).font(BOLD).color(AMBER_TEXT),
        ]
        .spacing(8)
        .align_y(Alignment::End),
        iced::widget::progress_bar(0.0..=1.0, pct).girth(8).style(|_| iced::widget::progress_bar::Style { background: rgba(0xffffff, 0.06).into(), bar: hex(0xf59e0b).into(), border: border(Color::TRANSPARENT, 0.0, 4.0) }),
        text(if s.target > 0.0 { format!("Goal {} {}", fmt_amount(s.target), s.symbol) } else { "Set a goal".to_string() }).size(12).color(TEXT_SECONDARY),
    ]
    .spacing(8);
    let stat = |label: &'static str, value: String| -> El<'a> {
        container(column![text(label).size(10.5).font(BOLD).color(FAINT), text(value).size(13).font(SEMIBOLD).color(TEXT_BODY)].spacing(3)).width(Fill).padding([10, 12]).style(boxed(rgba(0xffffff, 0.025), rgba(0xffffff, 0.05), 10.0)).into()
    };
    let stats = row![
        stat("BACKERS", s.backers.to_string()),
        stat("TIME", if s.deadline.is_empty() { "—".into() } else { s.deadline.clone() }),
        stat("ACCORDO FEE", s.fee.clone()),
    ]
    .spacing(8);
    let mut col = column![
        cover,
        column![text(s.title.clone()).size(20).font(BOLD).color(if s.preview && s.title == "Your campaign title" { FAINT } else { Color::WHITE }), who].spacing(8),
        raise,
        stats,
    ]
    .spacing(16);
    if !s.description.trim().is_empty() {
        col = col.push(text(crate::components::ui::truncate(&s.description, 420)).size(13).color(if s.preview { TEXT_SECONDARY } else { rgba(0xffffff, 0.82) }).line_height(1.5));
    }
    if s.preview {
        col = col.push(text("Preview — this is how backers will see it.").size(11.5).color(FAINT));
    }
    card(col)
}

/// Back tab, nothing picked yet: where campaigns come from.
fn pick_hint<'a>() -> El<'a> {
    card(
        column![
            container(icon(icons::compass(), 22, INDIGO_SOFT)).width(48).height(48).center(48).style(icon_tile(rgba(0x6366f1, 0.15))),
            text("Pick a campaign").size(17).font(BOLD).color(Color::WHITE),
            text("Paste its ID into the form, open one from a chat, or find one among the pools that are raising now.").size(13).color(TEXT_SECONDARY).line_height(1.5),
            button(row![icon(icons::globe(), 13, Color::WHITE), text("Explore Pools").size(12.5).font(SEMIBOLD)].spacing(6).align_y(Alignment::Center))
                .padding([7, 14])
                .style(tone_button(Tone::Accent))
                .on_press(msg(CrowdfundMsg::Tab(CrowdfundTab::Explore))),
        ]
        .spacing(10),
    )
}

/// Card header: tinted icon tile, title, subtitle, then a rule.
fn header<'a>(handle: svg::Handle, tint: u32, title: &'a str, subtitle: &'a str) -> El<'a> {
    column![
        row![
            container(icon(handle, 18, hex(tint))).width(38).height(38).center(38).style(icon_tile(rgba(tint, 0.15))),
            column![text(title).size(20).font(BOLD).color(Color::WHITE), text(subtitle).size(13).color(TEXT_SECONDARY)].spacing(2),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        container(space().width(Fill).height(1)).style(theme::rule),
    ]
    .spacing(16)
    .into()
}

fn card<'a>(content: impl Into<El<'a>>) -> El<'a> {
    container(content).width(Fill).padding(28).style(settings_card(false)).into()
}

fn launch_card<'a>(state: &'a State, tokens: token_select::Field, native: &'a str) -> El<'a> {
    let mut categories: Row<'a, Message> = row![].spacing(8);
    for (i, name) in CATEGORIES.iter().enumerate() {
        let active = state.category == i;
        categories = categories.push(
            button(text(*name).size(12).font(if active { BOLD } else { MEDIUM }))
                .padding([6, 14])
                .style(move |t, s| if active { tone_button(Tone::Accent)(t, s) } else { pill(false)(t, s) })
                .on_press(msg(CrowdfundMsg::Category(i))),
        );
    }

    let token_pick = token_select::compact(tokens, state.token.as_ref(), native, crate::components::token_picker::Purpose::Campaign);

    let model = |m: Model, title: &'a str, detail: &'a str| -> El<'a> {
        button(column![text(title).size(13).font(BOLD).color(Color::WHITE), text(detail).size(11.5).color(TEXT_SECONDARY).line_height(1.45)].spacing(4))
            .width(Fill)
            .padding([12, 14])
            .style(option_card(state.model == m))
            .on_press(msg(CrowdfundMsg::Model(m)))
            .into()
    };

    let steps = [
        "Backers contribute until the deadline. Funds sit in the Accordo smart contract — not with Accordo, not with you. Nothing is charged on the way in.",
        "Reached the target early? You can end the campaign right away and withdraw — no need to wait for the deadline.",
        "After the deadline you choose: withdraw everything raised, or refund all backers. The target is a guide, not a lock.",
        "You have 30 days to decide. If you don't, refunds open automatically so backers are never stuck.",
    ];
    let mut how: Column<'a, Message> = column![row![icon(icons::shield_check(), 14, INDIGO_SOFT), text("How it works").size(13).font(BOLD).color(Color::WHITE)].spacing(6).align_y(Alignment::Center)].spacing(8);
    for (i, step) in steps.iter().enumerate() {
        how = how.push(row![text(format!("{}.", i + 1)).size(12).font(BOLD).color(INDIGO_SOFT), text(*step).size(12).color(LABEL).line_height(1.45)].spacing(8));
    }

    card(
        column![
            header(icons::flame(), 0xf87171, "Launch a Campaign", "Raise crypto for your project, idea, or community goal."),
            column![label("Campaign Title", true), input("e.g. Decentralized AI Storage Network", &state.title, |v| msg(CrowdfundMsg::Title(v)))].spacing(8),
            column![label("Category", false), categories].spacing(8),
            column![label("Funding Target", true), row![input("0.0", &state.target, |v| msg(CrowdfundMsg::Target(v))), token_pick].spacing(10).align_y(Alignment::Center)].spacing(8),
            column![
                label("Funding model", false),
                row![
                    model(Model::Flexible, "Flexible", "Keep what you raise, even below the target. You decide after the deadline."),
                    model(Model::AllOrNothing, "All-or-nothing", "Withdraw only if the target is reached. Otherwise every backer is refunded."),
                ]
                .spacing(10),
            ]
            .spacing(8),
            container(how).width(Fill).padding([12, 14]).style(inset),
            duration_picker::view("Campaign duration", &DURATIONS, state.duration, None, |d| msg(CrowdfundMsg::Duration(d))),
            column![label("Campaign Pitch & Description", false), iced::widget::text_input("What is it for, what will the money do, when will backers hear from you?", &state.description).on_input(|v| msg(CrowdfundMsg::Description(v))).size(13.5).padding([14, 12]).style(theme::field)].spacing(8),
            fee_picker::view(
                "Platform fee (only when you withdraw)",
                state.fee_bps,
                Some("Taken from what you withdraw and sent to the Accordo treasury. Backers’ contributions go into the pool whole, and if the campaign ends in refunds they get every bit back — no fee is charged then."),
                true,
                |b| msg(CrowdfundMsg::Fee(b)),
            ),
            row![
                checkbox(state.agreed).size(14).style(tick).on_toggle(|v| msg(CrowdfundMsg::Agree(v))),
                column![
                    text("I've read the Crowdfunding Terms: I'll use the money as described, and I'm responsible for the campaign and any taxes and rules that apply to it").size(12.5).color(LABEL),
                    button(rich_text![span::<(), Font>("(Crowdfunding Terms)").underline(true).color(LINK)].size(12.5)).padding(0).style(link_button).on_press(msg(CrowdfundMsg::Terms)),
                ]
                .spacing(2),
            ]
            .spacing(10),
            button(iced::widget::center(row![icon(icons::flame(), 15, Color::WHITE), text("Launch campaign").size(14).font(SEMIBOLD)].spacing(8).align_y(Alignment::Center)).height(22))
                .width(Fill)
                .padding([11, 0])
                .style(tone_button(Tone::Accent))
                .on_press(msg(CrowdfundMsg::Launch)),
        ]
        .spacing(20),
    )
}

fn back_card<'a>(state: &'a State, native: &'a str) -> El<'a> {
    let picked = state.looked.as_ref().and_then(|r| r.as_ref().ok());
    let sym: &'a str = picked.map(|c| c.token_symbol.as_str()).unwrap_or(native);
    let gross: f64 = state.contribution.trim().replace(',', ".").parse().unwrap_or(0.0);
    // Current contracts take nothing on the way in (the fee is on withdrawals); older ones took
    // their fee from every contribution.
    let in_fee = picked.filter(|c| c.fee_on_withdrawal == Some(false)).map_or(0.0, |c| c.fee_bps as f64 / 10_000.0);
    let fee = gross * in_fee;
    let amount = |v: f64| if v == 0.0 { format!("0 {sym}") } else { format!("{} {sym}", (format!("{v:.8}").trim_end_matches('0').trim_end_matches('.'))) };

    let mut shares: Row<'a, Message> = row![].spacing(8);
    for pct in [25u8, 50, 75, 100] {
        let caption = if pct == 100 { "MAX".to_string() } else { format!("{pct}%") };
        shares = shares.push(button(iced::widget::center(text(caption).size(11).font(BOLD)).height(14)).width(Fill).padding([5, 0]).style(chip(false)).on_press(msg(CrowdfundMsg::Share(pct))));
    }

    let line = |left: El<'a>, right: El<'a>| -> El<'a> { row![left, space().width(Fill), right].align_y(Alignment::Center).into() };
    let summary = container(
        column![
            line(text("Gross Contribution:").size(12.5).color(TEXT_SECONDARY).into(), text(amount(gross)).size(12.5).font(BOLD).color(Color::WHITE).into()),
            line(
                row![icon(icons::percent(), 11, AMBER_TEXT), text(if in_fee > 0.0 { format!("Accordo fee ({}%, not refundable):", in_fee * 100.0) } else { "Accordo fee (taken only when the creator withdraws):".to_string() }).size(12.5).color(AMBER_TEXT)].spacing(4).align_y(Alignment::Center).into(),
                text(amount(fee)).size(12.5).font(BOLD).color(AMBER_TEXT).into(),
            ),
            line(text("Net to Campaign Pool:").size(14).font(SEMIBOLD).color(Color::WHITE).into(), text(amount(gross - fee)).size(14).font(BOLD).color(GREEN).into()),
            text("If the creator refunds after the deadline — or doesn't decide within 30 days — you get the pool amount back.").size(11).color(TEXT_SECONDARY),
        ]
        .spacing(8),
    )
    .width(Fill)
    .padding([14, 16])
    .style(inset);

    let terms = row![
        checkbox(state.understood).size(14).style(tick).on_toggle(|v| msg(CrowdfundMsg::Understood(v))),
        column![
            text("I understand this is a voluntary payment to the creator, not an investment, and Accordo doesn't guarantee the campaign").size(12.5).color(LABEL),
            button(rich_text![span::<(), Font>("(Crowdfunding Terms)").underline(true).color(LINK)].size(12.5)).padding(0).style(link_button).on_press(msg(CrowdfundMsg::Terms)),
        ]
        .spacing(2),
    ]
    .spacing(10);

    card(
        column![
            header(icons::coins(), 0x60a5fa, "Back a Campaign", "Support builders and community pools with instant on-chain settlement."),
            column![label("Select Campaign to Back", false), input("Paste 32-byte Campaign ID (0x...)", &state.campaign_id, |v| msg(CrowdfundMsg::CampaignId(v))), looked_line(state)].spacing(8),
            column![
                label("Contribution Amount", true),
                row![
                    input("0.0", &state.contribution, |v| msg(CrowdfundMsg::Contribution(v))),
                    container(text(sym).size(14).font(BOLD).color(Color::WHITE)).padding([10, 16]).style(boxed(hex(0x0e0f17), BORDER, 8.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                shares,
            ]
            .spacing(8),
            column![label("Backer Note / Encouragement (Optional)", false), input("e.g. Great vision! Let's build this together 🚀", &state.backer_note, |v| msg(CrowdfundMsg::BackerNote(v)))].spacing(8),
            summary,
            terms,
            button(iced::widget::center(row![icon(icons::flame(), 15, Color::WHITE), text("Back this campaign").size(14).font(SEMIBOLD)].spacing(8).align_y(Alignment::Center)).height(22))
                .width(Fill)
                .padding([11, 0])
                .style(tone_button(Tone::Accent))
                .on_press_maybe(state.understood.then(|| msg(CrowdfundMsg::Back))),
        ]
        .spacing(20),
    )
}

/// Under the id: which campaign it is, or why it isn't one.
fn looked_line<'a>(state: &'a State) -> El<'a> {
    match &state.looked {
        Some(Ok(c)) => row![icon(icons::circle_check(), 12, GREEN), text(format!("{} — its card is beside the form", c.title)).size(12).color(TEXT_SECONDARY)].spacing(6).align_y(Alignment::Center).into(),
        Some(Err(e)) => text(e.clone()).size(12).color(DANGER_TEXT).into(),
        None if !state.campaign_id.is_empty() && !(state.campaign_id.starts_with("0x") && state.campaign_id.len() == 66) => text("A campaign ID is 0x followed by 64 characters").size(12).color(TEXT_SECONDARY).into(),
        None => space().into(),
    }
}

fn status_of(c: &CampaignRead) -> (&'static str, Color) {
    match c.status {
        CampaignStatus::Active => ("ACTIVE", GREEN),
        CampaignStatus::Ended => ("ENDED · AWAITING DECISION", AMBER_TEXT),
        CampaignStatus::Claimed => ("WITHDRAWN", INDIGO_SOFT),
        CampaignStatus::Refunding => ("REFUNDING", DANGER_TEXT),
        CampaignStatus::Unknown => ("—", MUTED),
    }
}

fn deadline_text(c: &CampaignRead) -> String {
    let Some(t) = c.deadline.as_deref().and_then(crate::components::chat::display::local) else { return String::new() };
    let left = t.timestamp() - chrono::Utc::now().timestamp();
    if left > 0 && c.status == CampaignStatus::Active {
        let (d, h) = (left / 86_400, (left % 86_400) / 3_600);
        if d > 0 { format!("{d}d {h}h left") } else { format!("{h}h {}m left", (left % 3_600) / 60) }
    } else {
        format!("ended {}", t.format("%b %-d, %Y"))
    }
}

/// One campaign: title, who, progress, deadline, and what you can do with it.
pub fn campaign_card<'a>(c: &'a CampaignRead, actions: Vec<El<'a>>) -> El<'a> {
    let (label, tone) = status_of(c);
    let pct = if c.target_amount > 0.0 { (c.raised_amount / c.target_amount).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let chip = |t: String| -> El<'a> { container(text(t).size(10.5).color(TEXT_SECONDARY)).padding([2, 7]).style(boxed(rgba(0xffffff, 0.05), Color::TRANSPARENT, 5.0)).into() };
    let mut head = row![
        column![
            text(c.title.clone()).size(15).font(BOLD).color(Color::WHITE),
            row![text(format!("by @{}", c.creator_username)).size(12).color(TEXT_SECONDARY), chip(c.category.clone()), chip(c.chain_name.clone())].spacing(6).align_y(Alignment::Center),
        ]
        .spacing(4)
        .width(Fill),
        text(label).size(11).font(BOLD).color(tone),
    ]
    .align_y(Alignment::Center);
    if c.hidden == Some(true) {
        head = head.push(container(text("HIDDEN").size(10).font(BOLD).color(DANGER_TEXT)).padding([2, 6]));
    }
    let mut foot = row![
        text(format!("{} backer{}", c.contributors_count, if c.contributors_count == 1 { "" } else { "s" })).size(12).color(TEXT_SECONDARY),
        text("·").size(12).color(FAINT),
        text(deadline_text(c)).size(12).color(TEXT_SECONDARY),
        space().width(Fill),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    for a in actions {
        foot = foot.push(a);
    }
    container(
        column![
            head,
            column![
                iced::widget::progress_bar(0.0..=1.0, pct).girth(6).style(|_| iced::widget::progress_bar::Style { background: rgba(0xffffff, 0.06).into(), bar: hex(0xf59e0b).into(), border: border(Color::TRANSPARENT, 0.0, 3.0) }),
                row![
                    text(format!("{} {}", c.raised_amount, c.token_symbol)).size(13).font(BOLD).color(Color::WHITE),
                    text(format!("of {} {}", c.target_amount, c.token_symbol)).size(12).color(TEXT_SECONDARY),
                    space().width(Fill),
                    text(format!("{}%", (pct * 100.0).round())).size(12).font(BOLD).color(AMBER_TEXT),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ]
            .spacing(6),
            foot,
        ]
        .spacing(12),
    )
    .width(Fill)
    .padding(18)
    .style(settings_card(false))
    .into()
}

/// What the creator can do with their campaign now (My Pools and its window alike).
fn creator_actions<'a>(c: &CampaignRead) -> Vec<El<'a>> {
    let mut actions = Vec::new();
    match c.status {
        CampaignStatus::Active => {
            if c.goal_reached == Some(true) {
                actions.push(act(c, "End early", Action::EndEarly, false));
            }
            actions.push(act(c, "Refund everyone", Action::RefundAll, true));
        }
        CampaignStatus::Ended => {
            if c.is_all_or_nothing != Some(true) || c.goal_reached == Some(true) {
                actions.push(act(c, "Withdraw", Action::Withdraw, false));
            }
            actions.push(act(c, "Refund everyone", Action::RefundAll, true));
        }
        CampaignStatus::Claimed if c.net_raised_amount - c.claimed_amount > 1e-12 => actions.push(act(c, "Withdraw the rest", Action::Withdraw, false)),
        _ => {}
    }
    actions
}

/// A campaign card that opens the campaign's window when clicked (its buttons keep their own clicks).
fn openable<'a>(c: &CampaignRead, card: El<'a>) -> El<'a> {
    mouse_area(card).on_press(msg(CrowdfundMsg::Open(Box::new(c.clone())))).interaction(iced::mouse::Interaction::Pointer).into()
}

fn act<'a>(c: &CampaignRead, caption: &'a str, a: Action, danger: bool) -> El<'a> {
    button(text(caption).size(12).font(SEMIBOLD)).padding([6, 12]).style(tone_button(if danger { Tone::Danger } else { Tone::Dark })).on_press(msg(CrowdfundMsg::Act(Box::new(c.clone()), a))).into()
}

fn list_frame<'a>(title: &'a str, what: &'a str, body: El<'a>) -> El<'a> {
    column![
        row![
            column![text(title).size(20).font(BOLD).color(Color::WHITE), text(what).size(13).color(TEXT_SECONDARY)].spacing(4).width(Fill),
            button(row![icon(icons::refresh_cw(), 13, TEXT_BODY), text("Refresh").size(12.5)].spacing(6).align_y(Alignment::Center)).padding([6, 12]).style(tone_button(Tone::Dark)).on_press(msg(CrowdfundMsg::Refresh)),
        ]
        .align_y(Alignment::Center),
        body,
    ]
    .spacing(18)
    .into()
}

fn empty<'a>(title: &'a str, what: &'a str) -> El<'a> {
    container(column![icon(icons::flame(), 30, MUTED), text(title).size(15).font(BOLD).color(Color::WHITE), text(what).size(13).color(TEXT_SECONDARY)].spacing(8).align_x(Alignment::Center))
        .width(Fill)
        .center_x(Fill)
        .padding([48, 24])
        .style(boxed(rgba(0xffffff, 0.015), rgba(0xffffff, 0.04), 14.0))
        .into()
}

fn explore_view<'a>(state: &'a State, me: &'a str) -> El<'a> {
    let body: El = match &state.explore {
        None => container(text("Loading campaigns…").size(13).color(MUTED)).center_x(Fill).padding(40).into(),
        Some(Err(e)) => container(text(format!("Couldn't load campaigns: {e}")).size(13).color(DANGER_TEXT)).center_x(Fill).padding(40).into(),
        Some(Ok(list)) if list.is_empty() => empty("No live campaigns", "Launch one in Launch & Back — it shows up here."),
        Some(Ok(list)) => {
            let mut col: Column<'a, Message> = column![].spacing(12);
            for c in list {
                let mut actions = Vec::new();
                if c.status == CampaignStatus::Active && !c.creator_username.eq_ignore_ascii_case(me) && c.hidden != Some(true) {
                    actions.push(button(row![icon(icons::flame(), 12, Color::WHITE), text("Back").size(12).font(SEMIBOLD)].spacing(5).align_y(Alignment::Center)).padding([6, 14]).style(tone_button(Tone::Accent)).on_press(msg(CrowdfundMsg::Pick(c.campaign_id.clone()))).into());
                }
                actions.push(button(icon(icons::copy(), 12, MUTED)).padding(6).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(Message::CopyText(c.campaign_id.clone(), "Campaign ID")).into());
                col = col.push(openable(c, campaign_card(c, actions)));
            }
            col.into()
        }
    };
    list_frame("Explore Pools", "Live campaigns from the community.", body)
}

fn mine_view<'a>(state: &'a State) -> El<'a> {
    let body: El = match &state.mine {
        None => container(text("Loading your campaigns…").size(13).color(MUTED)).center_x(Fill).padding(40).into(),
        Some(Err(e)) => container(text(format!("Couldn't load your campaigns: {e}")).size(13).color(DANGER_TEXT)).center_x(Fill).padding(40).into(),
        Some(Ok(m)) if m.created.is_empty() && m.backed.is_empty() => empty("No campaigns yet", "Campaigns you launch or back show up here."),
        Some(Ok(m)) => {
            let mut col: Column<'a, Message> = column![].spacing(12);
            if !m.created.is_empty() {
                col = col.push(text("LAUNCHED BY YOU").size(10.5).font(BOLD).color(FAINT));
                for c in &m.created {
                    let mut actions = creator_actions(c);
                    actions.push(button(icon(icons::copy(), 12, MUTED)).padding(6).style(ghost(6.0, rgba(0xffffff, 0.06))).on_press(Message::CopyText(c.campaign_id.clone(), "Campaign ID")).into());
                    col = col.push(openable(c, campaign_card(c, actions)));
                }
            }
            if !m.backed.is_empty() {
                col = col.push(container(text("BACKED BY YOU").size(10.5).font(BOLD).color(FAINT)).padding(iced::Padding { top: 8.0, ..Default::default() }));
                for c in &m.backed {
                    let mut actions = Vec::new();
                    if c.status == CampaignStatus::Refunding {
                        actions.push(act(c, "Get my refund", Action::Refund, false));
                    }
                    if c.status == CampaignStatus::Active {
                        actions.push(button(text("Back again").size(12).font(SEMIBOLD)).padding([6, 12]).style(tone_button(Tone::Dark)).on_press(msg(CrowdfundMsg::Pick(c.campaign_id.clone()))).into());
                    }
                    col = col.push(openable(c, campaign_card(c, actions)));
                }
            }
            col.into()
        }
    };
    list_frame("My Pools", "Campaigns you launched or backed.", body)
}

/// Who a contribution is from: the account, the name its payer wrote (public page), or the wallet.
fn backer_name(b: &CampaignContributionRead) -> String {
    match (&b.contributor_username, b.contributor_name.as_deref().map(str::trim).filter(|n| !n.is_empty())) {
        (Some(u), _) if !u.is_empty() => format!("@{u}"),
        (_, Some(n)) => n.to_string(),
        _ => crate::wallets::short_address(&b.contributor_address),
    }
}

/// A date from the API, shown locally ("Nov 25, 2026, 14:05").
fn when(iso: Option<&str>) -> String {
    iso.and_then(crate::components::chat::display::local).map(|t| t.format("%b %-d, %Y, %H:%M").to_string()).unwrap_or_else(|| "—".into())
}

/// The campaign's window: on the left its cover, title, sections and what you can do; on the
/// right the story and numbers, or the backers and what they wrote.
fn campaign_window<'a>(o: &'a Opened, me: &'a str) -> El<'a> {
    let c = &o.campaign;
    let tint = cover_tint(&c.category);
    let (status, tone) = status_of(c);
    let mine = c.creator_username.eq_ignore_ascii_case(me);
    let backers_count = o.backers.as_ref().and_then(|r| r.as_ref().ok()).map_or(c.contributors_count as usize, Vec::len);

    let nav = |t: DetailTab, handle: svg::Handle, label: String| -> El<'a> {
        let active = o.tab == t;
        button(row![icon(handle, 15, if active { Color::WHITE } else { MUTED }), text(label).size(13).font(SEMIBOLD)].spacing(10).align_y(Alignment::Center))
            .width(Fill)
            .padding([9, 12])
            .style(selectable(active, rgba(0x6366f1, 0.22), 8.0))
            .on_press(msg(CrowdfundMsg::Detail(t)))
            .into()
    };
    let wide = |b: iced::widget::Button<'a, Message>| -> El<'a> { b.width(Fill).padding([9, 12]).into() };

    // What you can do with it now.
    let mut actions: Column<'a, Message> = column![].spacing(8);
    if mine {
        for a in creator_actions(c) {
            actions = actions.push(container(a).width(Fill));
        }
    } else if c.status == CampaignStatus::Active && c.hidden != Some(true) {
        actions = actions.push(wide(
            button(row![icon(icons::flame(), 14, Color::WHITE), text("Back this campaign").size(13).font(SEMIBOLD)].spacing(8).align_y(Alignment::Center))
                .style(tone_button(Tone::Accent))
                .on_press(msg(CrowdfundMsg::Pick(c.campaign_id.clone()))),
        ));
    } else if c.status == CampaignStatus::Refunding {
        actions = actions.push(container(act(c, "Get my refund", Action::Refund, false)).width(Fill));
    }
    let link_button = |handle: svg::Handle, label: &'static str, m: CrowdfundMsg| -> El<'a> {
        wide(button(row![icon(handle, 13, TEXT_BODY), text(label).size(12.5)].spacing(8).align_y(Alignment::Center)).style(tone_button(Tone::Dark)).on_press(msg(m)))
    };

    let side = container(
        column![
            container(
                row![
                    container(icon(icons::flame(), 22, Color::WHITE)).width(44).height(44).center(44).style(boxed(rgba(tint, 0.55), rgba(0xffffff, 0.15), 12.0)),
                    space().width(Fill),
                    container(text(status).size(10).font(BOLD).color(tone)).padding([3, 8]).style(boxed(rgba(0x000000, 0.35), rgba(0xffffff, 0.08), 999.0)),
                ]
                .align_y(Alignment::Start),
            )
            .width(Fill)
            .padding(12)
            .style(move |_| container::Style {
                background: Some(iced::Background::Gradient(iced::Gradient::Linear(iced::gradient::Linear::new(iced::Radians(2.4)).add_stop(0.0, rgba(tint, 0.45)).add_stop(1.0, rgba(tint, 0.06))))),
                border: border(rgba(tint, 0.3), 1.0, 12.0),
                ..Default::default()
            }),
            column![
                text(c.title.clone()).size(16).font(BOLD).color(Color::WHITE),
                text(format!("by @{} · {}", c.creator_username, c.chain_name)).size(11.5).color(TEXT_SECONDARY),
            ]
            .spacing(3),
            container(space().width(Fill).height(1)).style(theme::rule),
            column![nav(DetailTab::Story, icons::file_text(), "Story".into()), nav(DetailTab::Backers, icons::users(), format!("Backers ({backers_count})"))].spacing(4),
            space().height(Fill),
            actions,
            column![
                link_button(icons::copy(), "Copy public link", CrowdfundMsg::CopyLink(c.campaign_id.clone())),
                link_button(icons::external_link(), "Open public page", CrowdfundMsg::OpenPage(c.campaign_id.clone())),
            ]
            .spacing(6),
        ]
        .spacing(14),
    )
    .width(250)
    .height(Fill)
    .padding([18, 14])
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0));

    let (heading, sub) = match o.tab {
        DetailTab::Story => ("Story", "What it's for and how far it got"),
        DetailTab::Backers => ("Backers", "Who backed it and what they wrote"),
    };
    let header = row![
        column![text(heading).size(18).font(BOLD).color(Color::WHITE), text(sub).size(12).color(TEXT_SECONDARY)].spacing(2),
        space().width(Fill),
        button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(CrowdfundMsg::Close)),
    ]
    .align_y(Alignment::Center);
    let body = match o.tab {
        DetailTab::Story => story(c, mine),
        DetailTab::Backers => backers(o),
    };
    let main = column![
        container(header).padding(Padding { top: 20.0, right: 20.0, bottom: 14.0, left: 26.0 }),
        container(space().width(Fill).height(1)).style(theme::rule),
        scrollable(container(body).width(Fill).padding(Padding { top: 18.0, right: 26.0, bottom: 24.0, left: 26.0 })).height(Fill).style(thin_scroll),
    ]
    .width(Fill);
    container(row![side, main]).width(Length::Fixed(900.0)).height(Length::Fixed(620.0)).style(theme::modal).into()
}

/// The Story section: the raise, the numbers, where it stands, the pitch, the dates.
fn story<'a>(c: &'a CampaignRead, mine: bool) -> El<'a> {
    let pct = if c.target_amount > 0.0 { (c.raised_amount / c.target_amount).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let sym = c.token_symbol.as_str();
    let stat = |label: &'static str, value: String| -> El<'a> {
        container(column![text(label).size(10.5).font(BOLD).color(FAINT), text(value).size(13).font(SEMIBOLD).color(TEXT_BODY)].spacing(3)).width(Fill).padding([10, 12]).style(boxed(rgba(0xffffff, 0.025), rgba(0xffffff, 0.05), 10.0)).into()
    };
    let fee = c.fee_bps as f64 / 100.0;
    let mut col: Column<'a, Message> = column![
        row![
            chip_text(c.category.clone(), TEXT_SECONDARY),
            chip_text(if c.is_all_or_nothing == Some(true) { "All-or-nothing".into() } else { "Flexible".into() }, INDIGO_SOFT),
            chip_text(c.chain_name.clone(), TEXT_SECONDARY),
        ]
        .spacing(6),
        column![
            row![
                text(fmt_amount(c.raised_amount)).size(26).font(BOLD).color(Color::WHITE),
                text(format!("{sym} raised")).size(13).color(TEXT_SECONDARY),
                space().width(Fill),
                text(format!("{}%", (pct * 100.0).round())).size(16).font(BOLD).color(AMBER_TEXT),
            ]
            .spacing(8)
            .align_y(Alignment::End),
            iced::widget::progress_bar(0.0..=1.0, pct).girth(8).style(|_| iced::widget::progress_bar::Style { background: rgba(0xffffff, 0.06).into(), bar: hex(0xf59e0b).into(), border: border(Color::TRANSPARENT, 0.0, 4.0) }),
            text(format!("Goal {} {sym}", fmt_amount(c.target_amount))).size(12).color(TEXT_SECONDARY),
        ]
        .spacing(8),
        row![
            stat("BACKERS", c.contributors_count.to_string()),
            stat("TIME", deadline_text(c)),
            stat("ACCORDO FEE", if c.fee_on_withdrawal == Some(false) { format!("{fee}% per contribution") } else { format!("{fee}% on withdrawal") }),
            stat("IN THE POOL", format!("{} {sym}", fmt_amount((c.net_raised_amount - c.claimed_amount - c.refunded_amount).max(0.0)))),
        ]
        .spacing(8),
    ]
    .spacing(16);
    if let Some(banner) = outcome(c, mine) {
        col = col.push(banner);
    }
    if c.hidden == Some(true) {
        col = col.push(text(format!("Hidden by moderators{}", c.hidden_reason.as_deref().map(|r| format!(" — {r}")).unwrap_or_default())).size(12).color(DANGER_TEXT));
    }
    let pitch = c.description.as_deref().map(str::trim).filter(|d| !d.is_empty()).unwrap_or("No description.");
    col = col.push(text(pitch.to_string()).size(13).color(rgba(0xffffff, 0.82)).line_height(1.55));
    let line = |label: &'static str, value: String| -> El<'a> {
        row![text(label).size(12).color(TEXT_SECONDARY).width(Length::FillPortion(2)), text(value).size(12).color(TEXT_BODY).width(Length::FillPortion(3))].spacing(12).into()
    };
    col = col.push(
        container(
            column![
                line("Created", when(Some(c.created_at.as_str()))),
                line("Deadline", when(c.deadline.as_deref())),
                line("Decide by", when(c.decision_deadline.as_deref())),
                row![
                    text("Campaign ID").size(12).color(TEXT_SECONDARY).width(Length::FillPortion(2)),
                    row![
                        text(crate::wallets::short_address(&c.campaign_id)).size(12).font(Font::MONOSPACE).color(TEXT_BODY),
                        button(icon(icons::copy(), 12, MUTED)).padding(4).style(ghost(5.0, rgba(0xffffff, 0.06))).on_press(Message::CopyText(c.campaign_id.clone(), "Campaign ID")),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center)
                    .width(Length::FillPortion(3)),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(8),
        )
        .width(Fill)
        .padding([12, 14])
        .style(inset),
    );
    col.into()
}

fn chip_text<'a>(t: String, c: Color) -> El<'a> {
    container(text(t).size(11).color(c)).padding([3, 8]).style(boxed(rgba(0xffffff, 0.05), rgba(0xffffff, 0.06), 6.0)).into()
}

/// Where an ended campaign stands (the web's outcome banner).
fn outcome<'a>(c: &CampaignRead, mine: bool) -> Option<El<'a>> {
    let decide_by = when(c.decision_deadline.as_deref());
    let (words, color, bg) = match c.status {
        CampaignStatus::Ended if mine => (format!("The campaign has ended. Withdraw what was raised or refund everyone — decide by {decide_by}, after that refunds open automatically."), AMBER_TEXT, 0xf59e0b),
        CampaignStatus::Ended => (format!("The campaign has ended. The creator decides by {decide_by} whether to withdraw or refund; if they don't, refunds open automatically."), AMBER_TEXT, 0xf59e0b),
        CampaignStatus::Refunding => {
            let missed = c.is_all_or_nothing == Some(true) && c.goal_reached != Some(true) && c.claimed_amount == 0.0;
            let how = if c.fee_on_withdrawal == Some(false) { "Every backer can take back what reached the pool (the Accordo fee isn't returned)." } else { "Every backer can take back everything they contributed." };
            (format!("{}Refunds are open. {how}", if missed { "The target wasn't reached and this was an all-or-nothing campaign. " } else { "" }), DANGER_TEXT, 0xef4444)
        }
        CampaignStatus::Claimed => {
            let rest = if c.claimed_amount < c.net_raised_amount { format!(" of {} — the rest is still in the pool", fmt_amount(c.net_raised_amount)) } else { String::new() };
            (format!("The creator withdrew {} {}{rest}.", fmt_amount(c.claimed_amount), c.token_symbol), GREEN, 0x10b981)
        }
        _ => return None,
    };
    Some(container(text(words).size(12.5).color(color).line_height(1.45)).width(Fill).padding([12, 14]).style(boxed(rgba(bg, 0.08), rgba(bg, 0.3), 10.0)).into())
}

/// The Backers section: who gave what, and what they wrote.
fn backers<'a>(o: &'a Opened) -> El<'a> {
    let sym = o.campaign.token_symbol.as_str();
    match &o.backers {
        None => text("Loading backers…").size(13).color(MUTED).into(),
        Some(Err(e)) => text(format!("Couldn't load the backers: {e}")).size(13).color(DANGER_TEXT).into(),
        Some(Ok(list)) if list.is_empty() => empty("No backers yet", "Share the campaign's public page — anyone can back it there, even without an account."),
        Some(Ok(list)) => {
            let mut col: Column<'a, Message> = column![].spacing(8);
            for b in list {
                let mut item = column![row![
                    text(backer_name(b)).size(13).font(SEMIBOLD).color(Color::WHITE),
                    space().width(Fill),
                    text(format!("{} {sym}", fmt_amount(b.gross_amount))).size(13).font(BOLD).color(GREEN),
                ]
                .align_y(Alignment::Center)]
                .spacing(4);
                if let Some(m) = b.message.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
                    item = item.push(text(format!("“{m}”")).size(12.5).color(rgba(0xffffff, 0.75)).line_height(1.45));
                }
                item = item.push(text(when(Some(b.created_at.as_str()))).size(11).color(FAINT));
                col = col.push(container(item).width(Fill).padding([10, 14]).style(boxed(rgba(0xffffff, 0.025), rgba(0xffffff, 0.05), 10.0)));
            }
            col.into()
        }
    }
}

fn protocol_card<'a>() -> El<'a> {
    let mut points: Column<'a, Message> = column![].spacing(8);
    for point in [
        "Funds are held by the contract — only the creator can withdraw, only after the deadline",
        "Backers get refunds if the creator chooses so — or automatically after 30 days of silence",
        "Support for native ETH and ERC20 stablecoins",
    ] {
        points = points.push(row![icon(icons::circle_check(), 15, hex(0x10b981)), text(point).size(12.5).color(rgba(0xffffff, 0.8))].spacing(8).align_y(Alignment::Center));
    }
    card(
        column![
            row![
                container(icon(icons::sparkles(), 18, AMBER_TEXT)).width(38).height(38).center(38).style(icon_tile(rgba(0xf59e0b, 0.15))),
                text("Accordo Crowdfund Protocol").size(17).font(BOLD).color(Color::WHITE),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            text("A zero-custody Web3 funding infrastructure. Funds are secured by smart contracts on EVM networks.").size(13).color(TEXT_SECONDARY).line_height(1.5),
            points,
        ]
        .spacing(14),
    )
}
