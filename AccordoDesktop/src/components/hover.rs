// SPDX-License-Identifier: GPL-3.0-or-later
//! Floating extras of one chat message, Discord style:
//!
//! - a toolbar over its top-right corner while the pointer is on the message (or on the toolbar);
//! - a menu (context menu, emoji picker) where it was right-clicked, or under the toolbar.
//!
//! Both float above everything (iced overlays), so they take no room in the feed. The hover is the
//! widget's own state: moving the mouse over the chat doesn't go through the app at all.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{tree, Operation, Tree};
use iced::advanced::{mouse, overlay, renderer, Clipboard, Shell, Widget};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

#[derive(Default)]
struct State {
    hovered: bool,
    /// Where the base was right-clicked (layout coordinates).
    clicked_at: Option<Point>,
    /// The toolbar's last bounds (window coordinates): the pointer on it keeps it open.
    toolbar: Option<Rectangle>,
}

pub struct Hover<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    base: Element<'a, Message, Theme, Renderer>,
    toolbar: Option<Element<'a, Message, Theme, Renderer>>,
    menu: Option<Element<'a, Message, Theme, Renderer>>,
    /// Keep the toolbar shown (a menu opened from it is up).
    pinned: bool,
    on_right_click: Option<Message>,
    on_dismiss: Option<Message>,
    /// Background while hovered (or while its menu is open).
    highlight: Option<iced::Color>,
    /// Privacy: the content is veiled until the pointer is on it (the gutter stays visible).
    conceal: Option<(f32, iced::Color)>,
}

impl<'a, Message, Theme, Renderer> Hover<'a, Message, Theme, Renderer> {
    pub fn new(base: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self { base: base.into(), toolbar: None, menu: None, pinned: false, on_right_click: None, on_dismiss: None, highlight: None, conceal: None }
    }

    pub fn toolbar(mut self, toolbar: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        self.toolbar = Some(toolbar.into());
        self
    }

    /// The menu shown now (None: closed), with what a click outside it sends.
    pub fn menu(mut self, menu: Option<Element<'a, Message, Theme, Renderer>>, on_dismiss: Message) -> Self {
        self.menu = menu;
        self.on_dismiss = Some(on_dismiss);
        self
    }

    pub fn pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        self
    }

    pub fn highlight(mut self, color: iced::Color) -> Self {
        self.highlight = Some(color);
        self
    }

    /// Veils everything right of `left` px with `color` until hovered (blur chat content).
    pub fn conceal(mut self, on: bool, left: f32, color: iced::Color) -> Self {
        self.conceal = on.then_some((left, color));
        self
    }

    pub fn on_right_click(mut self, message: Message) -> Self {
        self.on_right_click = Some(message);
        self
    }
}

impl<Message: Clone, Theme, Renderer> Widget<Message, Theme, Renderer> for Hover<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    // Children: base, toolbar, menu — always three slots, so trees don't shift when the menu opens.
    fn children(&self) -> Vec<Tree> {
        vec![
            Tree::new(&self.base),
            self.toolbar.as_ref().map(Tree::new).unwrap_or_else(Tree::empty),
            self.menu.as_ref().map(Tree::new).unwrap_or_else(Tree::empty),
        ]
    }

    fn diff(&self, tree: &mut Tree) {
        if tree.children.len() != 3 {
            tree.children = self.children();
            return;
        }
        tree.children[0].diff(&self.base);
        match &self.toolbar {
            Some(t) => tree.children[1].diff(t),
            None => tree.children[1] = Tree::empty(),
        }
        match &self.menu {
            Some(m) => tree.children[2].diff(m),
            None => tree.children[2] = Tree::empty(),
        }
    }

    fn size(&self) -> Size<Length> {
        self.base.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.base.as_widget().size_hint()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        self.base.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.base.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        // Only a free pointer counts: one levitating is under a window on top of us (a modal, a
        // docked layer). Over an overlay (our own toolbar) the base gets no pointer at all, so
        // the toolbar is checked against the raw position.
        let on_base = matches!(cursor, mouse::Cursor::Available(p) if bounds.contains(p) && viewport.contains(p));
        if let Event::Mouse(mouse::Event::CursorMoved { position }) = event {
            let on_toolbar = matches!(cursor, mouse::Cursor::Unavailable) && state.toolbar.is_some_and(|t| t.expand(4.0).contains(*position));
            let hovered = on_base || (state.hovered && on_toolbar);
            if hovered != state.hovered {
                state.hovered = hovered;
                shell.invalidate_layout();
                shell.request_redraw();
            }
        }
        if let (Event::Mouse(mouse::Event::CursorLeft), true) = (event, state.hovered) {
            state.hovered = false;
            shell.invalidate_layout();
            shell.request_redraw();
        }
        if let (Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)), Some(msg)) = (event, &self.on_right_click) {
            if cursor.is_over(bounds) {
                state.clicked_at = cursor.position();
                shell.publish(msg.clone());
                shell.capture_event();
                return;
            }
        }
        if self.menu.is_none() {
            state.clicked_at = None;
        }
        self.base.as_widget_mut().update(&mut tree.children[0], event, layout, cursor, renderer, clipboard, shell, viewport);
    }

    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer) -> mouse::Interaction {
        self.base.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn draw(&self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) {
        let state = tree.state.downcast_ref::<State>();
        if let Some(color) = self.highlight.filter(|_| state.hovered || self.pinned || self.menu.is_some()) {
            renderer.fill_quad(renderer::Quad { bounds: layout.bounds(), border: iced::Border { radius: 6.0.into(), ..Default::default() }, ..Default::default() }, color);
        }
        let Some((left, color)) = self.conceal.filter(|_| !(state.hovered || self.pinned || self.menu.is_some())) else {
            self.base.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
            return;
        };
        // Concealed: only the gutter (avatar) is drawn — the content isn't drawn at all, so
        // nothing shows through — and a frosted card with a few soft "lines" takes its place.
        let b = layout.bounds();
        let gutter = Rectangle { width: left.min(b.width), ..b };
        renderer.with_layer(gutter.intersection(viewport).unwrap_or(Rectangle::new(Point::ORIGIN, Size::ZERO)), |r| {
            self.base.as_widget().draw(&tree.children[0], r, theme, style, layout, cursor, viewport);
        });
        let veil = Rectangle { x: b.x + left, y: b.y + 3.0, width: (b.width - left - 6.0).max(0.0), height: (b.height - 6.0).max(0.0) };
        let round = |radius: f32| iced::Border { radius: radius.into(), ..Default::default() };
        renderer.fill_quad(renderer::Quad { bounds: veil, border: round(8.0), ..Default::default() }, color);
        let lines = ((veil.height - 12.0) / 16.0).floor().clamp(1.0, 4.0) as usize;
        for i in 0..lines {
            let w = veil.width * [0.62, 0.84, 0.48, 0.7][i % 4];
            let bar = Rectangle { x: veil.x + 12.0, y: veil.y + 10.0 + i as f32 * 16.0, width: (w - 24.0).max(0.0), height: 7.0 };
            if bar.y + bar.height < veil.y + veil.height - 4.0 {
                renderer.fill_quad(renderer::Quad { bounds: bar, border: round(4.0), ..Default::default() }, iced::Color { a: 0.06, ..iced::Color::WHITE });
            }
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let bounds = layout.bounds();
        let (base_tree, rest) = tree.children.split_first_mut()?;
        let (toolbar_tree, rest) = rest.split_first_mut()?;
        let menu_tree = rest.first_mut()?;
        let state = tree.state.downcast_mut::<State>();
        let mut layers = vec![];
        if let Some(inner) = self.base.as_widget_mut().overlay(base_tree, layout, renderer, viewport, translation) {
            layers.push(inner);
        }
        let show_toolbar = (state.hovered || self.pinned || self.menu.is_some()) && bounds.intersects(viewport);
        match (&mut self.toolbar, show_toolbar) {
            (Some(toolbar), true) => {
                let slot = &mut state.toolbar;
                layers.push(overlay::Element::new(Box::new(Floating {
                    content: toolbar,
                    tree: toolbar_tree,
                    place: Place::TopRight(bounds + translation),
                    remember: Some(slot),
                    on_dismiss: None,
                })));
            }
            _ => state.toolbar = None,
        }
        if let Some(menu) = &mut self.menu {
            let place = match state.clicked_at {
                Some(p) => Place::At(p + translation),
                None => Place::BelowRight(bounds + translation),
            };
            layers.push(overlay::Element::new(Box::new(Floating { content: menu, tree: menu_tree, place, remember: None, on_dismiss: self.on_dismiss.clone() })));
        }
        if layers.is_empty() {
            None
        } else {
            Some(overlay::Group::with_children(layers).overlay())
        }
    }
}

#[derive(Clone, Copy)]
enum Place {
    /// The toolbar: over the top-right corner of the message.
    TopRight(Rectangle),
    /// Under the toolbar, right-aligned with the message.
    BelowRight(Rectangle),
    /// At the pointer.
    At(Point),
}

struct Floating<'a, 'b, Message, Theme, Renderer> {
    content: &'b mut Element<'a, Message, Theme, Renderer>,
    tree: &'b mut Tree,
    place: Place,
    /// Where it ended up (layout coordinates), for the hover check.
    remember: Option<&'b mut Option<Rectangle>>,
    on_dismiss: Option<Message>,
}

impl<Message: Clone, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer> for Floating<'_, '_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let limits = layout::Limits::new(Size::ZERO, bounds);
        let node = self.content.as_widget_mut().layout(self.tree, renderer, &limits);
        let size = node.size();
        let (x, y) = match self.place {
            Place::TopRight(r) => (r.x + r.width - size.width - 16.0, r.y - size.height * 0.55),
            Place::BelowRight(r) => (r.x + r.width - size.width - 16.0, r.y + 22.0),
            Place::At(p) => (p.x + 2.0, p.y + 2.0),
        };
        // Stay inside the window: flip up/left near the edges.
        let x = x.clamp(4.0, (bounds.width - size.width - 4.0).max(4.0));
        let y = if y + size.height > bounds.height - 4.0 {
            match self.place {
                Place::At(p) => (p.y - size.height - 2.0).max(4.0),
                _ => (bounds.height - size.height - 4.0).max(4.0),
            }
        } else {
            y.max(4.0)
        };
        if let Some(slot) = self.remember.as_mut() {
            **slot = Some(Rectangle::new(Point::new(x, y), size));
        }
        node.move_to(Point::new(x, y))
    }

    fn draw(&self, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor) {
        let bounds = layout.bounds();
        self.content.as_widget().draw(self.tree, renderer, theme, style, layout, cursor, &bounds);
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.content.as_widget_mut().operate(self.tree, layout, renderer, operation);
    }

    fn update(&mut self, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer, clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Message>) {
        let bounds = layout.bounds();
        if let (Event::Mouse(mouse::Event::ButtonPressed(_)), Some(dismiss)) = (event, &self.on_dismiss) {
            if !cursor.is_over(bounds) {
                shell.publish(dismiss.clone());
                return;
            }
        }
        self.content.as_widget_mut().update(self.tree, event, layout, cursor, renderer, clipboard, shell, &bounds);
        if cursor.is_over(bounds) {
            if let Event::Mouse(mouse::Event::ButtonPressed(_) | mouse::Event::WheelScrolled { .. }) = event {
                shell.capture_event();
            }
        }
    }

    fn mouse_interaction(&self, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(self.tree, layout, cursor, &layout.bounds(), renderer)
    }

    fn overlay<'c>(&'c mut self, layout: Layout<'c>, renderer: &Renderer) -> Option<overlay::Element<'c, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(self.tree, layout, renderer, &layout.bounds(), Vector::ZERO)
    }
}

impl<'a, Message: Clone + 'a, Theme: 'a, Renderer: iced::advanced::Renderer + 'a> From<Hover<'a, Message, Theme, Renderer>> for Element<'a, Message, Theme, Renderer> {
    fn from(h: Hover<'a, Message, Theme, Renderer>) -> Self {
        Element::new(h)
    }
}
