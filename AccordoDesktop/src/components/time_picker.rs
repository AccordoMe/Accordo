// SPDX-License-Identifier: GPL-3.0-or-later
//! The date & time window: a month calendar, the time, quick picks — one window for the chat and
//! for the forms, which differ only in what they get back:
//! - the chat gets a `<t:unix:style>` token (the formats are on the right, with previews);
//! - a cheque's expiry, an invoice's due date, a campaign's end, a drop's start get the unix time
//!   (on the right: what was picked, how far away, in the person's timezone).
//!
//! The forms only take moments in the future.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone, Timelike};
use iced::widget::{button, column, container, row, space, text, text_input, Column, Row};
use iced::{Alignment, Color, Fill, Length, Padding};

use crate::app::Message;
use crate::components::chat::timestamp;
use crate::components::overlay;
use crate::components::ui::{icon, El};
use crate::icons;
use crate::theme::*;

pub const TIME_FIELD: &str = "time-picker-clock";

/// Who asked, and so what the pick turns into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// A token for the message box.
    Chat,
    ChequeExpiry,
    InvoiceDue,
    #[cfg(feature = "crowdfund")]
    CampaignEnd,
    /// For the drop form (the desktop has no drop form yet).
    #[allow(dead_code)]
    #[cfg(feature = "drops")]
    DropStart,
}

impl Purpose {
    fn title(self) -> &'static str {
        match self {
            Purpose::Chat => "Insert time",
            Purpose::ChequeExpiry => "Cheque expiry",
            Purpose::InvoiceDue => "Payment due date",
            #[cfg(feature = "crowdfund")]
            Purpose::CampaignEnd => "Campaign end",
            #[cfg(feature = "drops")]
            Purpose::DropStart => "Drop start",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Purpose::Chat => "Everyone sees it in their own timezone",
            Purpose::ChequeExpiry => "After it, unclaimed money can be refunded",
            Purpose::InvoiceDue => "After it, the invoice shows as overdue",
            #[cfg(feature = "crowdfund")]
            Purpose::CampaignEnd => "Contributions close at this moment",
            #[cfg(feature = "drops")]
            Purpose::DropStart => "Claims open at this moment",
        }
    }

    fn confirm(self) -> &'static str {
        match self {
            Purpose::Chat => "Insert",
            Purpose::ChequeExpiry => "Set expiry",
            Purpose::InvoiceDue => "Set due date",
            #[cfg(feature = "crowdfund")]
            Purpose::CampaignEnd => "Set end",
            #[cfg(feature = "drops")]
            Purpose::DropStart => "Set start",
        }
    }

    /// The forms need a moment ahead (a minute's margin, as the web checks).
    fn future_only(self) -> bool {
        self != Purpose::Chat
    }
}

#[derive(Debug, Clone)]
pub enum TimeMsg {
    Open(Purpose, Option<i64>),
    Close,
    Month(i32),
    Day(NaiveDate),
    Clock(String),
    Step(i64),
    At(i64),
    Style(char),
    Confirm,
}

pub struct State {
    pub purpose: Purpose,
    /// The month on screen (its first day).
    pub month: NaiveDate,
    pub day: NaiveDate,
    pub clock: String,
    /// Chat only: the token's style.
    pub style: char,
}

/// The next full hour.
fn next_hour() -> DateTime<Local> {
    let at = Local::now() + Duration::hours(1);
    at.with_minute(0).and_then(|t| t.with_second(0)).unwrap_or(at)
}

fn first_of(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

impl State {
    /// `at`: what the field holds already, if anything.
    pub fn new(purpose: Purpose, at: Option<i64>) -> Self {
        let t = at.and_then(|u| Local.timestamp_opt(u, 0).single()).unwrap_or_else(next_hour);
        Self { purpose, month: first_of(t.date_naive()), day: t.date_naive(), clock: t.format("%H:%M").to_string(), style: 'C' }
    }

    fn set(&mut self, t: DateTime<Local>) {
        self.day = t.date_naive();
        self.month = first_of(self.day);
        self.clock = t.format("%H:%M").to_string();
    }

    fn time(&self) -> Option<NaiveTime> {
        NaiveTime::parse_from_str(self.clock.trim(), "%H:%M").ok()
    }

    /// The moment picked, if it's a real one (a time that doesn't exist on a DST day isn't).
    pub fn picked(&self) -> Option<i64> {
        Local.from_local_datetime(&self.day.and_time(self.time()?)).earliest().map(|t| t.timestamp())
    }

    /// Why it can't be used yet, if so.
    pub fn problem(&self, now: i64) -> Option<&'static str> {
        let Some(at) = self.picked() else {
            return Some(if self.time().is_none() { "Type the time as HH:MM" } else { "That time doesn't exist on this day (clock change)" });
        };
        (self.purpose.future_only() && at <= now + 60).then_some("Pick a moment in the future")
    }

    pub fn update(&mut self, m: TimeMsg) {
        match m {
            TimeMsg::Month(d) => {
                let (y, mo) = (self.month.year(), self.month.month() as i32 - 1 + d);
                let (y, mo) = (y + mo.div_euclid(12), mo.rem_euclid(12) as u32 + 1);
                if let Some(m) = NaiveDate::from_ymd_opt(y, mo, 1) {
                    self.month = m;
                }
            }
            TimeMsg::Day(d) => {
                self.day = d;
                if first_of(d) != self.month {
                    self.month = first_of(d);
                }
            }
            TimeMsg::Clock(v) => self.clock = v,
            TimeMsg::Step(minutes) => {
                let base = self.picked().and_then(|u| Local.timestamp_opt(u, 0).single()).unwrap_or_else(Local::now);
                self.set(base + Duration::minutes(minutes));
            }
            TimeMsg::At(u) => {
                if let Some(t) = Local.timestamp_opt(u, 0).single() {
                    self.set(t);
                }
            }
            TimeMsg::Style(s) => self.style = s,
            TimeMsg::Open(..) | TimeMsg::Close | TimeMsg::Confirm => {}
        }
    }
}

fn msg(m: TimeMsg) -> Message {
    Message::TimePicker(m)
}

fn quick(purpose: Purpose) -> Vec<(&'static str, i64)> {
    let now = Local::now();
    let at = |d: NaiveDate, h: u32| d.and_hms_opt(h, 0, 0).and_then(|n| Local.from_local_datetime(&n).earliest()).unwrap_or(now).timestamp();
    let tomorrow = now.date_naive() + Duration::days(1);
    let monday = now.date_naive() + Duration::days(7 - i64::from(now.weekday().num_days_from_monday()));
    let mut v = vec![("+15 min", (now + Duration::minutes(15)).timestamp()), ("+1 hour", (now + Duration::hours(1)).timestamp()), ("Tomorrow 9:00", at(tomorrow, 9)), ("Monday 9:00", at(monday, 9))];
    if purpose != Purpose::Chat {
        v.retain(|(l, _)| *l != "+15 min");
        v.extend([("In 1 week", (now + Duration::weeks(1)).timestamp()), ("In 1 month", (now + Duration::days(30)).timestamp()), ("In 1 year", (now + Duration::days(365)).timestamp())]);
    }
    v
}

fn calendar<'a>(st: &'a State, now: DateTime<Local>) -> El<'a> {
    let today = now.date_naive();
    let head = row![
        button(icon(icons::chevron_right(), 14, MUTED).rotation(std::f32::consts::PI)).padding(6).style(ghost(7.0, rgba(0xffffff, 0.06))).on_press(msg(TimeMsg::Month(-1))),
        container(text(st.month.format("%B %Y").to_string()).size(15).font(BOLD).color(Color::WHITE)).center_x(Fill),
        button(icon(icons::chevron_right(), 14, MUTED)).padding(6).style(ghost(7.0, rgba(0xffffff, 0.06))).on_press(msg(TimeMsg::Month(1))),
    ]
    .align_y(Alignment::Center);
    let mut grid: Column<'a, Message> = column![].spacing(4);
    let mut names: Row<'a, Message> = row![].spacing(4);
    for n in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
        names = names.push(container(text(n).size(11).font(BOLD).color(FAINT)).width(Fill).center_x(Fill));
    }
    grid = grid.push(names);
    let lead = i64::from(st.month.weekday().num_days_from_monday());
    let start = st.month - Duration::days(lead);
    for w in 0..6 {
        let mut week: Row<'a, Message> = row![].spacing(4);
        for d in 0..7 {
            let day = start + Duration::days(w * 7 + d);
            let inside = day.month() == st.month.month();
            let picked = day == st.day;
            let is_today = day == today;
            let past = st.purpose.future_only() && day < today;
            let color = if picked { Color::WHITE } else if past { rgba(0xffffff, 0.18) } else if inside { TEXT_BODY } else { FAINT };
            let label = text(day.day().to_string()).size(13).font(if picked || is_today { BOLD } else { INTER }).color(color);
            let cell = button(container(label).center_x(Fill).center_y(Fill)).width(Fill).height(38).padding(0).style(move |t, s| {
                if picked {
                    button::Style { background: Some(INDIGO.into()), text_color: Color::WHITE, border: border(Color::TRANSPARENT, 0.0, 8.0), ..Default::default() }
                } else {
                    let mut b = ghost(8.0, rgba(0xffffff, 0.06))(t, s);
                    if is_today {
                        b.border = border(rgba(0x6366f1, 0.6), 1.0, 8.0);
                    }
                    b
                }
            });
            week = week.push(cell.on_press_maybe((!past).then(|| msg(TimeMsg::Day(day)))));
        }
        grid = grid.push(week);
    }
    column![head, grid].spacing(12).into()
}

fn clock<'a>(st: &'a State) -> El<'a> {
    let ok = st.time().is_some();
    let step = |label: &'a str, m: i64| -> El<'a> { button(text(label).size(11.5).font(SEMIBOLD)).padding([5, 9]).style(pill(false)).on_press(msg(TimeMsg::Step(m))).into() };
    let mut presets = row![].spacing(4);
    for (l, h, mi) in [("09:00", 9, 0), ("12:00", 12, 0), ("18:00", 18, 0), ("23:59", 23, 59)] {
        let t = NaiveTime::from_hms_opt(h, mi, 0).unwrap_or_default();
        let at = Local.from_local_datetime(&st.day.and_time(t)).earliest().map(|x| x.timestamp());
        presets = presets.push(button(text(l).size(11.5)).padding([5, 9]).style(pill(st.time() == Some(t))).on_press_maybe(at.map(|a| msg(TimeMsg::At(a)))));
    }
    column![
        row![
            icon(icons::clock(), 14, MUTED),
            text_input("HH:MM", &st.clock)
                .id(TIME_FIELD)
                .on_input(|v| msg(TimeMsg::Clock(v)))
                .on_submit(msg(TimeMsg::Confirm))
                .size(15)
                .padding([7, 10])
                .width(90)
                .style(move |t: &iced::Theme, s| {
                    let mut f = field(t, s);
                    if !ok {
                        f.border.color = DANGER_TEXT;
                    }
                    f
                }),
            step("−1h", -60),
            step("−15m", -15),
            step("+15m", 15),
            step("+1h", 60),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
        presets,
    ]
    .spacing(8)
    .into()
}

/// Chat: the formats with a preview of each.
fn formats<'a>(st: &'a State, picked: Option<i64>) -> El<'a> {
    let mut col: Column<'a, Message> = column![text("FORMAT").size(10).font(BOLD).color(FAINT)].spacing(2);
    for (s, label) in timestamp::STYLES {
        let preview = picked.map(|u| timestamp::format(u, if s == 'C' { 'R' } else { s })).unwrap_or_else(|| "—".into());
        col = col.push(
            button(column![text(label).size(13).font(if st.style == s { BOLD } else { SEMIBOLD }).color(Color::WHITE), text(preview).size(11.5).color(TEXT_SECONDARY)].spacing(2))
                .width(Fill)
                .padding([8, 12])
                .style(selectable(st.style == s, rgba(0x6366f1, 0.18), 8.0))
                .on_press(msg(TimeMsg::Style(s))),
        );
    }
    col.into()
}

/// The forms: what was picked, in words.
fn summary<'a>(picked: Option<i64>, now: i64) -> El<'a> {
    let Some(u) = picked.filter(|u| *u > now) else {
        return container(text("Pick a day and a time ahead.").size(13).color(MUTED)).padding([12, 0]).into();
    };
    let t = Local.timestamp_opt(u, 0).single();
    let line = |label: &'a str, value: String| -> El<'a> { row![text(label).size(12.5).color(TEXT_SECONDARY).width(110), text(value).size(13).color(TEXT_BODY)].spacing(10).into() };
    let left = u - now;
    let span = if left >= 86_400 { format!("{} days {} h", left / 86_400, (left % 86_400) / 3_600) } else { format!("{} h {} min", left / 3_600, (left % 3_600) / 60) };
    column![
        text("PICKED").size(10).font(BOLD).color(FAINT),
        text(t.map(|t| t.format("%A, %B %-d, %Y").to_string()).unwrap_or_default()).size(17).font(BOLD).color(Color::WHITE),
        text(t.map(|t| t.format("%H:%M").to_string()).unwrap_or_default()).size(28).font(BOLD).color(INDIGO_SOFT),
        space().height(6),
        line("From now", span),
        line("Your timezone", t.map(|t| t.format("UTC%:z").to_string()).unwrap_or_default()),
        line("In UTC", chrono::DateTime::from_timestamp(u, 0).map(|t| t.format("%b %-d, %Y %H:%M").to_string()).unwrap_or_default()),
        line("Unix time", u.to_string()),
    ]
    .spacing(6)
    .into()
}

pub fn view<'a>(st: &'a State) -> El<'a> {
    let now = Local::now();
    let picked = st.picked();
    let problem = st.problem(now.timestamp());

    let mut quicks = Row::new().spacing(6);
    for (l, at) in quick(st.purpose) {
        quicks = quicks.push(button(text(l).size(11.5).font(SEMIBOLD)).padding([5, 10]).style(pill(false)).on_press(msg(TimeMsg::At(at))));
    }
    let left = container(
        column![
            row![
                container(icon(icons::calendar_clock(), 18, INDIGO_SOFT)).width(36).height(36).center(36).style(boxed(rgba(0x6366f1, 0.14), rgba(0x6366f1, 0.3), 10.0)),
                column![text(st.purpose.title()).size(16).font(BOLD).color(Color::WHITE), text(st.purpose.subtitle()).size(11.5).color(TEXT_SECONDARY)].spacing(2),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            calendar(st, now),
            clock(st),
            quicks.wrap(),
        ]
        .spacing(18),
    )
    .width(440)
    .height(Fill)
    .padding([20, 20])
    .style(boxed(hex(0x0f1016), Color::TRANSPARENT, 14.0));

    let body: El = if st.purpose == Purpose::Chat { formats(st, picked) } else { summary(picked, now.timestamp()) };
    let footer = row![
        text(problem.unwrap_or("")).size(12).color(DANGER_TEXT),
        space().width(Fill),
        button(text("Cancel").size(13)).padding([8, 16]).style(tone_button(Tone::Dark)).on_press(msg(TimeMsg::Close)),
        button(text(st.purpose.confirm()).size(13).font(SEMIBOLD)).padding([8, 18]).style(tone_button(Tone::Accent)).on_press_maybe(problem.is_none().then(|| msg(TimeMsg::Confirm))),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let main = column![
        container(row![space().width(Fill), button(icon(icons::x(), 18, MUTED)).padding(6).style(ghost(8.0, rgba(0xffffff, 0.06))).on_press(msg(TimeMsg::Close))]).padding(Padding { top: 14.0, right: 14.0, bottom: 0.0, left: 0.0 }),
        iced::widget::scrollable(container(body).width(Fill).padding(Padding { top: 0.0, right: 26.0, bottom: 16.0, left: 26.0 })).height(Fill).style(thin_scroll),
        container(space().width(Fill).height(1)).style(rule),
        container(footer).padding([12, 20]),
    ]
    .width(Fill);
    let window = container(row![left, main]).width(Length::Fixed(860.0)).height(Length::Fixed(620.0)).style(modal);
    overlay::modal(window, msg(TimeMsg::Close))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_roll_over() {
        let mut st = State::new(Purpose::Chat, None);
        st.month = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        st.update(TimeMsg::Month(1));
        assert_eq!(st.month, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
        st.update(TimeMsg::Month(-2));
        assert_eq!(st.month, NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
    }

    #[test]
    fn forms_want_the_future() {
        let now = Local::now().timestamp();
        let mut st = State::new(Purpose::ChequeExpiry, Some(now - 3_600));
        assert_eq!(st.problem(now), Some("Pick a moment in the future"));
        st.update(TimeMsg::At(now + 86_400));
        assert!(st.problem(now).is_none());
        st.clock = "25:00".into();
        assert_eq!(st.problem(now), Some("Type the time as HH:MM"));
        // The chat takes the past too.
        let chat = State::new(Purpose::Chat, Some(now - 3_600));
        assert!(chat.problem(now).is_none());
    }

    #[test]
    fn steps_move_the_clock() {
        let now = Local::now().timestamp();
        let mut st = State::new(Purpose::Chat, Some(now));
        let before = st.picked().unwrap();
        st.update(TimeMsg::Step(60));
        assert_eq!(st.picked().unwrap() - before, 3_600);
    }
}
