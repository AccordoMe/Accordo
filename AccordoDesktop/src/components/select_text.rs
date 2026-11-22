// SPDX-License-Identifier: GPL-3.0-or-later
//! Text you can select with the mouse and copy, as in the web (iced's own text can't be
//! selected): message bodies. Drag to select, double-click a word, triple-click a line, Ctrl+C
//! copies; a click anywhere else, or Esc, clears it.
//!
//! Built like iced's rich text (spans: emoji in their font, time chips), but with one paragraph
//! per line of the text, so a point on screen always maps to a line and a byte in it. The
//! selection is drawn as a highlight on the selected part of the spans.

use std::borrow::Cow;
use std::time::{Duration, Instant};

use iced::advanced::clipboard::Kind;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{Alignment as TextAlign, Highlight, Paragraph as _, Renderer as _};
use iced::advanced::widget::{tree, Tree, Widget};
use iced::advanced::{Clipboard, Renderer as _, Shell, Text};
use iced::keyboard::key::{Code, Named, Physical};
use iced::widget::text::{LineHeight, Shaping, Span, Wrapping};
use iced::{alignment, keyboard, mouse, Border, Color, Element, Event, Length, Pixels, Point, Rectangle, Size, Vector};

type Paragraph = <iced::Renderer as iced::advanced::text::Renderer>::Paragraph;
/// A place in the text: (line, byte in that line).
type Pos = (usize, usize);

/// The fields' selection colour (theme::field).
const SELECTION: Color = Color { r: 0x58 as f32 / 255.0, g: 0x65 as f32 / 255.0, b: 0xf2 as f32 / 255.0, a: 0.45 };
const MULTI_CLICK: Duration = Duration::from_millis(450);

pub struct SelectText<'a> {
    /// The spans, split at line breaks (no span holds a '\n').
    lines: Vec<Vec<Span<'a, ()>>>,
    size: Pixels,
    line_height: LineHeight,
}

/// Selectable text from spans (each with its own size / colour / font, as for `rich_text`).
pub fn view<'a, M: 'a>(spans: Vec<Span<'a, ()>>, size: f32, line_height: f32) -> Element<'a, M> {
    SelectText { lines: split_lines(spans), size: Pixels(size), line_height: LineHeight::Relative(line_height) }.into()
}

fn split_lines(spans: Vec<Span<'_, ()>>) -> Vec<Vec<Span<'_, ()>>> {
    let mut lines = vec![Vec::new()];
    for span in spans {
        if !span.text.contains('\n') {
            lines.last_mut().expect("one line at least").push(span);
            continue;
        }
        for (i, part) in span.text.split('\n').enumerate() {
            if i > 0 {
                lines.push(Vec::new());
            }
            if !part.is_empty() {
                let mut piece = span.clone();
                piece.text = Cow::Owned(part.to_string());
                lines.last_mut().expect("one line at least").push(piece);
            }
        }
    }
    lines
}

struct Line {
    text: String,
    /// As laid out: the source spans, the selected part highlighted.
    spans: Vec<Span<'static, ()>>,
    paragraph: Paragraph,
    y: f32,
    height: f32,
}

#[derive(Default)]
struct State {
    /// The spans the lines were built from; new ones (another message here) clear the selection.
    source: Vec<Vec<Span<'static, ()>>>,
    lines: Vec<Line>,
    width: f32,
    /// The selection the lines were built with.
    built: Option<(Pos, Pos)>,
    anchor: Option<Pos>,
    head: Option<Pos>,
    dragging: bool,
    /// When and where the last press was, and how many in a row (1–3).
    last_click: Option<(Instant, Point, u8)>,
}

impl State {
    /// The selection, start first; None when nothing is selected.
    fn ordered(&self) -> Option<(Pos, Pos)> {
        let (a, b) = (self.anchor?, self.head?);
        (a != b).then(|| if a < b { (a, b) } else { (b, a) })
    }

    fn clear(&mut self) -> bool {
        let had = self.anchor.is_some();
        self.anchor = None;
        self.head = None;
        self.dragging = false;
        had
    }

    /// The place under a point (relative to the widget); outside it, the nearest one.
    fn hit(&self, p: Point) -> Pos {
        let Some(last) = self.lines.len().checked_sub(1) else { return (0, 0) };
        if p.y < 0.0 {
            return (0, 0);
        }
        let end = &self.lines[last];
        if p.y >= end.y + end.height {
            return (last, end.text.len());
        }
        let i = self.lines.iter().position(|l| p.y < l.y + l.height).unwrap_or(last);
        let l = &self.lines[i];
        if p.x <= 0.0 {
            return (i, 0);
        }
        let local = Point::new(p.x, (p.y - l.y).clamp(0.0, (l.height - 1.0).max(0.0)));
        let at = l.paragraph.hit_test(local).map_or(l.text.len(), |h| h.cursor());
        (i, boundary(&l.text, at))
    }

    fn selected_text(&self) -> Option<String> {
        let (a, b) = self.ordered()?;
        let mut out = String::new();
        for (i, l) in self.lines.iter().enumerate().take(b.0 + 1).skip(a.0) {
            let t = &l.text;
            let (s, e) = range_in(i, a, b, t.len());
            if i > a.0 {
                out.push('\n');
            }
            out.push_str(&t[s..e]);
        }
        Some(out).filter(|s| !s.is_empty())
    }
}

/// The nearest char boundary at or before `at`.
fn boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The part of line `i` inside the selection a..b, as bytes.
fn range_in(i: usize, a: Pos, b: Pos, len: usize) -> (usize, usize) {
    let s = if i == a.0 { a.1.min(len) } else { 0 };
    let e = if i == b.0 { b.1.min(len) } else { len };
    (s.min(e), e)
}

/// The word around a byte (letters, digits, _), or the one character there.
fn word(text: &str, at: usize) -> (usize, usize) {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let at = boundary(text, at);
    let here = text[at..].chars().next();
    let before = text[..at].chars().next_back();
    match (here, before) {
        (Some(c), _) if is_word(c) => {}
        (_, Some(c)) if is_word(c) => {}
        (Some(c), _) => return (at, at + c.len_utf8()),
        (None, Some(c)) => return (at - c.len_utf8(), at),
        (None, None) => return (at, at),
    }
    let start = text[..at].char_indices().rev().take_while(|(_, c)| is_word(*c)).last().map_or(at, |(i, _)| i);
    let end = at + text[at..].chars().take_while(|c| is_word(*c)).map(char::len_utf8).sum::<usize>();
    (start, end)
}

/// The spans with bytes s..e of their text highlighted (split where the selection starts / ends).
fn highlighted(spans: &[Span<'static, ()>], range: Option<(usize, usize)>) -> Vec<Span<'static, ()>> {
    let Some((s, e)) = range.filter(|(s, e)| s < e) else { return spans.to_vec() };
    let mut out = Vec::with_capacity(spans.len() + 2);
    let mut at = 0;
    for span in spans {
        let t = span.text.as_ref();
        let n = t.len();
        let (a, b) = (s.clamp(at, at + n) - at, e.clamp(at, at + n) - at);
        for (x, y, on) in [(0, a, false), (a, b, true), (b, n, false)] {
            if x < y {
                let mut piece = span.clone();
                piece.text = Cow::Owned(t[x..y].to_string());
                if on {
                    piece.highlight = Some(Highlight { background: SELECTION.into(), border: Border::default() });
                }
                out.push(piece);
            }
        }
        at += n;
    }
    out
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for SelectText<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        let st = tree.state.downcast_mut::<State>();
        let width = limits.max().width;
        let same = st.source.len() == self.lines.len() && st.source.iter().zip(&self.lines).all(|(a, b)| a.as_slice() == b.as_slice());
        if !same {
            st.source = self.lines.iter().map(|l| l.iter().cloned().map(Span::to_static).collect()).collect();
            st.clear();
            st.lines.clear();
        }
        let sel = st.ordered();
        let empty = self.line_height.to_absolute(self.size).0;
        // Only narrower / wider (the window resized): wrap the same paragraphs again.
        if st.lines.len() == st.source.len() && st.built == sel && st.width != width {
            let mut y = 0.0;
            for l in &mut st.lines {
                l.paragraph.resize(Size::new(width, f32::INFINITY));
                l.y = y;
                l.height = if l.text.is_empty() { empty } else { l.paragraph.min_bounds().height.max(empty) };
                y += l.height;
            }
            st.width = width;
        }
        if st.lines.len() != st.source.len() || st.width != width || st.built != sel {
            let font = renderer.default_font();
            let mut y = 0.0;
            let mut lines = Vec::with_capacity(st.source.len());
            for (i, source) in st.source.iter().enumerate() {
                let len = source.iter().map(|s| s.text.len()).sum();
                let range = sel.filter(|(a, b)| a.0 <= i && i <= b.0).map(|(a, b)| range_in(i, a, b, len));
                let spans = highlighted(source, range);
                let text: String = spans.iter().map(|s| s.text.as_ref()).collect();
                let paragraph = Paragraph::with_spans(Text {
                    content: spans.as_slice(),
                    bounds: Size::new(width, f32::INFINITY),
                    size: self.size,
                    line_height: self.line_height,
                    font,
                    align_x: TextAlign::Default,
                    align_y: alignment::Vertical::Top,
                    shaping: Shaping::Advanced,
                    wrapping: Wrapping::WordOrGlyph,
                });
                let height = if text.is_empty() { empty } else { paragraph.min_bounds().height.max(empty) };
                lines.push(Line { text, spans, paragraph, y, height });
                y += height;
            }
            st.lines = lines;
            st.width = width;
            st.built = sel;
        }
        let w = st.lines.iter().map(|l| l.paragraph.min_bounds().width).fold(0.0, f32::max);
        let h = st.lines.last().map_or(0.0, |l| l.y + l.height);
        layout::Node::new(limits.resolve(Length::Shrink, Length::Shrink, Size::new(w.ceil(), h)))
    }

    fn draw(&self, tree: &Tree, renderer: &mut iced::Renderer, _theme: &iced::Theme, style: &renderer::Style, layout: Layout<'_>, _cursor: mouse::Cursor, viewport: &Rectangle) {
        let bounds = layout.bounds();
        if !bounds.intersects(viewport) {
            return;
        }
        let st = tree.state.downcast_ref::<State>();
        for line in &st.lines {
            let origin = Vector::new(bounds.x, bounds.y + line.y);
            for (i, span) in line.spans.iter().enumerate() {
                let Some(h) = span.highlight else { continue };
                for r in line.paragraph.span_bounds(i) {
                    let r = Rectangle::new(r.position() - Vector::new(span.padding.left, span.padding.top), r.size() + Size::new(span.padding.x(), span.padding.y()));
                    renderer.fill_quad(Quad { bounds: r + origin, border: h.border, ..Default::default() }, h.background);
                }
            }
            renderer.fill_paragraph(&line.paragraph, Point::ORIGIN + origin, style.text_color, *viewport);
        }
    }

    fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, _renderer: &iced::Renderer, clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, M>, _viewport: &Rectangle) {
        let st = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let changed = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => match cursor.position_in(bounds) {
                Some(p) => {
                    let now = Instant::now();
                    let clicks = match st.last_click {
                        Some((t, q, n)) if now.duration_since(t) < MULTI_CLICK && q.distance(p) < 5.0 => n % 3 + 1,
                        _ => 1,
                    };
                    st.last_click = Some((now, p, clicks));
                    let pos = st.hit(p);
                    let len = st.lines.get(pos.0).map_or(0, |l| l.text.len());
                    let (from, to) = match clicks {
                        2 => {
                            let (s, e) = word(&st.lines[pos.0].text, pos.1);
                            ((pos.0, s), (pos.0, e))
                        }
                        3 => ((pos.0, 0), (pos.0, len)),
                        _ => (pos, pos),
                    };
                    st.anchor = Some(from);
                    st.head = Some(to);
                    st.dragging = clicks == 1;
                    true
                }
                None => st.clear(),
            },
            Event::Mouse(mouse::Event::CursorMoved { position }) if st.dragging => {
                let pos = st.hit(*position - Vector::new(bounds.x, bounds.y));
                let moved = st.head != Some(pos);
                st.head = Some(pos);
                moved
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                st.dragging = false;
                false
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, physical_key, modifiers, .. }) => {
                let copy = modifiers.command() && (matches!(key.as_ref(), keyboard::Key::Character("c")) || *physical_key == Physical::Code(Code::KeyC));
                match st.selected_text() {
                    Some(t) if copy => {
                        clipboard.write(Kind::Standard, t);
                        shell.capture_event();
                        false
                    }
                    Some(_) if matches!(key, keyboard::Key::Named(Named::Escape)) => st.clear(),
                    _ => false,
                }
            }
            _ => false,
        };
        if changed {
            shell.invalidate_layout();
            shell.request_redraw();
        }
    }

    fn mouse_interaction(&self, _tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, _viewport: &Rectangle, _renderer: &iced::Renderer) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) { mouse::Interaction::Text } else { mouse::Interaction::None }
    }
}

impl<'a, M: 'a> From<SelectText<'a>> for Element<'a, M> {
    fn from(t: SelectText<'a>) -> Self {
        Element::new(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_lines() {
        let t = "привет, world_1!";
        assert_eq!(&t[word(t, 2).0..word(t, 2).1], "привет"); // a byte inside "п"
        let at = t.find("world").unwrap() + 2;
        assert_eq!(&t[word(t, at).0..word(t, at).1], "world_1");
        let comma = t.find(',').unwrap();
        assert_eq!(word(t, comma), (0, comma)); // right after a word (its last letter's right half): the word
        assert_eq!(word(t, comma + 1), (comma + 1, comma + 2)); // between "," and " ": the space
        assert_eq!(word("", 0), (0, 0));
        let lines = split_lines(vec![Span::new("a\nb"), Span::new("c\n"), Span::new("\nd")]);
        let texts: Vec<String> = lines.iter().map(|l| l.iter().map(|s| s.text.as_ref()).collect()).collect();
        assert_eq!(texts, ["a", "bc", "", "d"]);
    }

    #[test]
    fn highlight_splits_spans() {
        let spans = vec![Span::new("hello ").to_static(), Span::new("world").to_static()];
        let out = highlighted(&spans, Some((3, 8)));
        let parts: Vec<(&str, bool)> = out.iter().map(|s| (s.text.as_ref(), s.highlight.is_some())).collect();
        assert_eq!(parts, [("hel", false), ("lo ", true), ("wo", true), ("rld", false)]);
        assert_eq!(highlighted(&spans, Some((2, 2))).len(), 2);
    }

    #[test]
    fn copies_across_lines() {
        let line = |t: &str, y: f32| Line { text: t.into(), spans: vec![], paragraph: Paragraph::default(), y, height: 10.0 };
        let mut st = State { lines: vec![line("first", 0.0), line("", 10.0), line("third", 20.0)], ..Default::default() };
        st.anchor = Some((2, 3));
        st.head = Some((0, 2));
        assert_eq!(st.selected_text().as_deref(), Some("rst\n\nthi"));
        st.head = st.anchor;
        assert_eq!(st.selected_text(), None);
        assert_eq!(st.hit(Point::new(5.0, -3.0)), (0, 0));
        assert_eq!(st.hit(Point::new(5.0, 99.0)), (2, 5));
        assert_eq!(st.hit(Point::new(-1.0, 12.0)), (1, 0));
    }
}
