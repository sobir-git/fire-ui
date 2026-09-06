use crate::Point;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}
impl Modifiers {
    pub fn command(self) -> bool {
        self.control || self.meta
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,
    Character(char),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Preview,
    Target,
    Bubble,
}
#[derive(Clone, Debug)]
pub enum Input {
    Pointer {
        pointer: u32,
        position: Point,
    },
    Button {
        pointer: u32,
        button: u16,
        down: bool,
        position: Point,
    },
    Scroll {
        position: Point,
        delta: Point,
    },
    Key {
        key: Key,
        physical: u32,
        down: bool,
        repeat: bool,
        modifiers: Modifiers,
    },
    Text {
        session: u64,
        text: String,
    },
    Preedit {
        session: u64,
        text: String,
    },
}
impl Input {
    pub fn position(&self) -> Option<Point> {
        match self {
            Self::Pointer { position, .. }
            | Self::Button { position, .. }
            | Self::Scroll { position, .. } => Some(*position),
            _ => None,
        }
    }
    pub(crate) fn local(&self, t: crate::Transform) -> Self {
        let mut input = self.clone();
        match &mut input {
            Self::Pointer { position, .. }
            | Self::Button { position, .. }
            | Self::Scroll { position, .. } => *position = t.point(*position),
            _ => {}
        }
        input
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Mount,
    Resized,
    Unmount,
    Visibility(bool),
    Focus(bool),
    CaptureLost(u32),
    CancelKeys,
    Hover(bool),
    Anchor(bool),
}
