// SPDX-License-Identifier: GPL-3.0-or-later
//! A column that takes no room of its own: it shows as many of its rows as fit in the space its
//! parent has left, whole rows only, and hides the rest. Put last in a settings card: the card
//! keeps the height its row of cards gives it (EqualRow measures it without these rows), and the
//! rows fill what's free below the rest of the card.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{mouse, renderer, Clipboard, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size};

pub struct FitColumn<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    children: Vec<Element<'a, Message, Theme, Renderer>>,
    spacing: f32,
    /// Rows that fit at the last layout (the first `shown`).
    shown: usize,
    /// The first child is a title: shown only with at least one row under it.
    titled: bool,
}

impl<'a, Message, Theme, Renderer> FitColumn<'a, Message, Theme, Renderer> {
    pub fn new(children: Vec<Element<'a, Message, Theme, Renderer>>, spacing: f32) -> Self {
        Self { children, spacing, shown: 0, titled: false }
    }

    /// The first child is the rows' title (hidden when no row fits).
    pub fn titled(mut self) -> Self {
        self.titled = true;
        self
    }
}

/// How many rows of these heights fit in `room` with `spacing` between them.
pub fn fitting(heights: &[f32], spacing: f32, room: f32) -> usize {
    let mut used = 0.0;
    for (i, h) in heights.iter().enumerate() {
        let next = used + if i == 0 { 0.0 } else { spacing } + h;
        if next > room + 0.5 {
            return i;
        }
        used = next;
    }
    heights.len()
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for FitColumn<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn size(&self) -> Size<Length> {
        Size { width: Length::Fill, height: Length::Shrink }
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let width = limits.max().width;
        let room = limits.max().height;
        // Measured with no limit (EqualRow's first pass): no rows, no height.
        if !room.is_finite() {
            self.shown = 0;
            let nodes = self.children.iter().map(|_| layout::Node::new(Size::ZERO)).collect();
            return layout::Node::with_children(Size::new(width, 0.0), nodes);
        }
        let free = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
        let mut nodes: Vec<layout::Node> = self.children.iter_mut().zip(&mut tree.children).map(|(c, s)| c.as_widget_mut().layout(s, renderer, &free)).collect();
        let heights: Vec<f32> = nodes.iter().map(|n| n.size().height).collect();
        self.shown = fitting(&heights, self.spacing, room);
        if self.titled && self.shown < 2 {
            self.shown = 0;
        }
        let mut y = 0.0;
        for (i, node) in nodes.iter_mut().enumerate() {
            if i < self.shown {
                if i > 0 {
                    y += self.spacing;
                }
                *node = node.clone().move_to((0.0, y));
                y += heights[i];
            } else {
                *node = layout::Node::new(Size::ZERO);
            }
        }
        layout::Node::with_children(Size::new(width, y), nodes)
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        let shown = self.shown;
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
                .take(shown)
                .for_each(|((child, state), layout)| child.as_widget_mut().operate(state, layout, renderer, operation));
        });
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
        let shown = self.shown;
        for ((child, state), layout) in self.children.iter_mut().zip(&mut tree.children).zip(layout.children()).take(shown) {
            child.as_widget_mut().update(state, event, layout, cursor, renderer, clipboard, shell, viewport);
        }
    }

    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .take(self.shown)
            .map(|((child, state), layout)| child.as_widget().mouse_interaction(state, layout, cursor, viewport, renderer))
            .max()
            .unwrap_or_default()
    }

    fn draw(&self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) {
        for ((child, state), layout) in self.children.iter().zip(&tree.children).zip(layout.children()).take(self.shown) {
            child.as_widget().draw(state, renderer, theme, style, layout, cursor, viewport);
        }
    }
}

impl<'a, Message: 'a, Theme: 'a, Renderer: iced::advanced::Renderer + 'a> From<FitColumn<'a, Message, Theme, Renderer>> for Element<'a, Message, Theme, Renderer> {
    fn from(c: FitColumn<'a, Message, Theme, Renderer>) -> Self {
        Element::new(c)
    }
}

#[cfg(test)]
mod tests {
    use super::fitting;

    #[test]
    fn whole_rows_only() {
        assert_eq!(fitting(&[40.0, 40.0, 40.0], 8.0, 1000.0), 3);
        assert_eq!(fitting(&[40.0, 40.0, 40.0], 8.0, 88.0), 2); // 40 + 8 + 40
        assert_eq!(fitting(&[40.0, 40.0, 40.0], 8.0, 87.0), 1); // the second doesn't fit whole
        assert_eq!(fitting(&[40.0], 8.0, 10.0), 0);
        assert_eq!(fitting(&[], 8.0, 10.0), 0);
    }
}
