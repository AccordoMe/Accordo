// SPDX-License-Identifier: GPL-3.0-or-later
//! A row of equal-width cells that all take the height of the tallest one — CSS grid's
//! `align-items: stretch`, as the web's settings grid does. iced's row can't do it inside a
//! scrollable (Fill height there has nothing to fill), so: measure every cell, then lay them out
//! again with that height as their minimum (Shrink cells grow to it; their backgrounds fill it).

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{mouse, overlay, renderer, Clipboard, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

pub struct EqualRow<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    children: Vec<Element<'a, Message, Theme, Renderer>>,
    spacing: f32,
}

impl<'a, Message, Theme, Renderer> EqualRow<'a, Message, Theme, Renderer> {
    pub fn new(children: Vec<Element<'a, Message, Theme, Renderer>>, spacing: f32) -> Self {
        Self { children, spacing }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for EqualRow<'_, Message, Theme, Renderer>
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
        let n = self.children.len().max(1) as f32;
        let width = limits.max().width;
        let cell = ((width - self.spacing * (n - 1.0)) / n).max(0.0);

        // 1. Natural heights.
        let free = layout::Limits::new(Size::new(cell, 0.0), Size::new(cell, f32::INFINITY));
        let tallest = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .map(|(child, state)| child.as_widget_mut().layout(state, renderer, &free).size().height)
            .fold(0.0f32, f32::max);

        // 2. Everyone at the tallest one's height.
        let fixed = layout::Limits::new(Size::new(cell, tallest), Size::new(cell, tallest));
        let nodes = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .enumerate()
            .map(|(i, (child, state))| child.as_widget_mut().layout(state, renderer, &fixed).move_to((i as f32 * (cell + self.spacing), 0.0)))
            .collect();
        layout::Node::with_children(Size::new(width, tallest), nodes)
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
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
        for ((child, state), layout) in self.children.iter_mut().zip(&mut tree.children).zip(layout.children()) {
            child.as_widget_mut().update(state, event, layout, cursor, renderer, clipboard, shell, viewport);
        }
    }

    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| child.as_widget().mouse_interaction(state, layout, cursor, viewport, renderer))
            .max()
            .unwrap_or_default()
    }

    fn draw(&self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) {
        for ((child, state), layout) in self.children.iter().zip(&tree.children).zip(layout.children()).filter(|(_, l)| l.bounds().intersects(viewport)) {
            child.as_widget().draw(state, renderer, theme, style, layout, cursor, viewport);
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
        overlay::from_children(&mut self.children, tree, layout, renderer, viewport, translation)
    }
}

impl<'a, Message: 'a, Theme: 'a, Renderer: iced::advanced::Renderer + 'a> From<EqualRow<'a, Message, Theme, Renderer>> for Element<'a, Message, Theme, Renderer> {
    fn from(row: EqualRow<'a, Message, Theme, Renderer>) -> Self {
        Element::new(row)
    }
}
