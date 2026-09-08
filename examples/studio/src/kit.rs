//! Small conveniences the studio builds on. Everything here is ordinary use of
//! the public API; nothing reaches into framework internals.

use fire_ui::*;
use fire_ui_widgets::*;

/// Text at a step of the type scale, in the theme's ordinary foreground.
pub fn text(content: impl Into<std::sync::Arc<str>>, role: TextRole) -> Element<Label> {
    Element::leaf(Label::styled(content, role))
}
/// Secondary text: captions, hints and supporting lines.
pub fn muted(content: impl Into<std::sync::Arc<str>>, role: TextRole) -> Element<Label> {
    Element::leaf(Label::toned(content, role, ColorRole::Muted))
}
/// Secondary text that wraps.
pub fn muted_paragraph(content: impl Into<std::sync::Arc<str>>, role: TextRole) -> Element<Label> {
    Element::leaf(Label::toned(content, role, ColorRole::Muted).wrap())
}
/// Text in the accent, for the one word that should catch the eye.
pub fn accent(content: impl Into<std::sync::Arc<str>>, role: TextRole) -> Element<Label> {
    Element::leaf(Label::toned(content, role, ColorRole::Accent))
}

/// A heading and its supporting line, above content. The studio's unit of teaching.
pub struct Section<C: Widget> {
    title: Child<Label>,
    caption: Child<Label>,
    content: Child<C>,
}
impl<C: Widget> Section<C> {
    pub fn new(title: &str, caption: &str, content: Element<C>) -> Element<Self> {
        Element::build(|children| Self {
            title: children.add(text(title.to_string(), TextRole::Heading)),
            caption: children.add(muted_paragraph(caption.to_string(), TextRole::Small)),
            content: children.bubble(content),
        })
    }
}
impl<C: Widget> Widget for Section<C> {
    type Command = C::Command;
    type Output = C::Output;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Self::Command) {
        let _ = cx.send(self.content, command);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        column(
            cx,
            c,
            Flow::gap(gap(cx, 0.75)).align(Align::Stretch),
            &[
                Entry::natural(self.title),
                Entry::natural(self.caption),
                Entry::natural(self.content).aligned(Align::Stretch),
            ],
        )
    }
}

/// A labelled row: a caption on the left, a control on the right. Used wherever the
/// studio shows one control at a time.
pub struct Field<C: Widget> {
    caption: Child<Label>,
    control: Child<C>,
}
impl<C: Widget> Field<C> {
    pub fn new(caption: &str, control: Element<C>) -> Element<Self> {
        Element::build(|children| Self {
            caption: children.add(muted(caption.to_string(), TextRole::Small)),
            control: children.bubble(control),
        })
    }
}
impl<C: Widget> Widget for Field<C> {
    type Command = C::Command;
    type Output = C::Output;
    fn update(&mut self, cx: &mut Update<'_, Self>, command: Self::Command) {
        let _ = cx.send(self.control, command);
    }
    fn layout(&mut self, cx: &mut Layout<'_>, c: Constraints) -> Metrics {
        column(
            cx,
            c,
            Flow::gap(gap(cx, 0.5)).align(Align::Stretch),
            &[
                Entry::natural(self.caption),
                Entry::natural(self.control).aligned(Align::Stretch),
            ],
        )
    }
}
