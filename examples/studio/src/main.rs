//! The notes exercise, standard controls, and custom game use one widget runtime.
mod experiments;
use experiments::{FireText, Rally};
use fire_ui::*;
use fire_ui_native::*;
use fire_ui_widgets::*;
use std::time::Duration;

const INTRO: &str = "A small place to think.\n\nThis editor is a widget, just like the buttons and the game beside it.\n\nClick here and write. Select text, move the caret, paste a paragraph, or undo an edit. Long lines wrap using the same positions as the caret and selection.\n\nThe framework stays asleep until something needs to change. Try the fire and the game, then pause them.\n\nBuilt from a persistent tree. Drawn with a small set of shapes, paths, and text runs.";

struct Studio {
    editor: WidgetId,
    note_list: WidgetId,
    status: WidgetId,
    palette: WidgetId,
    open: WidgetId,
    close: WidgetId,
    fire: WidgetId,
    animate: WidgetId,
    count: usize,
}
impl Studio {
    fn new() -> Self {
        Self {
            editor: WidgetId::new(),
            note_list: WidgetId::new(),
            status: WidgetId::new(),
            palette: WidgetId::new(),
            open: WidgetId::new(),
            close: WidgetId::new(),
            fire: WidgetId::new(),
            animate: WidgetId::new(),
            count: 0,
        }
    }
}
fn label(text: &str, size: f32, color: Color) -> Node {
    Node::new(Label::new(text, color, size))
}
fn column(gap: f32, lengths: Vec<Length>, children: Vec<Node>) -> Node {
    Node::new(Flex::column(gap).lengths(lengths)).children(children)
}
impl NativeApp for Studio {
    fn build(&mut self) -> Node {
        let t = Theme::default();
        let header = Node::new(Flex::row(20.).lengths([Length::Fill(1.), Length::Fixed(150.)]))
            .children([
                column(
                    2.,
                    vec![Length::Fixed(32.), Length::Fixed(22.)],
                    vec![
                        label("FIRE / UI", 27., t.foreground),
                        label(
                            "A native interface, built from the smallest parts.",
                            13.,
                            t.muted,
                        ),
                    ],
                ),
                Node::new(Button::new("Open palette", t)).with_id(self.open),
            ]);
        let left = column(
            16.,
            vec![Length::Fixed(20.), Length::Fill(1.), Length::Fixed(65.)],
            vec![
                label("01 / NOTES", 12., t.accent),
                Node::new(List::new(
                    [
                        "Scratchpad",
                        "Composition",
                        "Text & selection",
                        "Frame scheduling",
                        "Custom drawing",
                        "Game canvas",
                    ]
                    .map(str::to_string),
                    t,
                ))
                .with_id(self.note_list),
                label("Up / Down to select\nEnter to open", 12., t.muted),
            ],
        );
        let mut title = TextInput::new("An ordinary widget.", t);
        title.placeholder = "Note title".into();
        let mut search = TextInput::new("", t);
        search.placeholder = "A second independent input".into();
        let editor = TextInput::editor(INTRO, t);
        let center = column(
            12.,
            vec![
                Length::Fixed(20.),
                Length::Fixed(49.),
                Length::Fixed(49.),
                Length::Fill(1.),
            ],
            vec![
                label("02 / COMPOSE", 12., t.accent),
                Node::new(title),
                Node::new(search),
                Node::new(editor).with_id(self.editor),
            ],
        );
        let right = column(
            12.,
            vec![
                Length::Fixed(20.),
                Length::Fixed(170.),
                Length::Fixed(40.),
                Length::Fixed(20.),
                Length::Fill(1.),
                Length::Fixed(22.),
            ],
            vec![
                label("03 / BREAK THE RULES", 12., t.accent),
                Node::new(FireText::new(t)).with_id(self.fire),
                Node::new(Button::new("Animate / pause", t)).with_id(self.animate),
                label("A WIDGET CAN BE A GAME", 11., t.muted),
                Node::new(Rally::new(t)),
                label("Click to play. Move to return the ball.", 11., t.muted),
            ],
        );
        let body = Node::new(Flex::row(24.).lengths([
            Length::Fixed(185.),
            Length::Fill(1.),
            Length::Fixed(285.),
        ]))
        .children([left, center, right]);
        let content = Node::new(Panel {
            color: t.background,
            padding: 28.,
            radius: 0.,
        })
        .child(column(
            22.,
            vec![Length::Fixed(58.), Length::Fill(1.), Length::Fixed(20.)],
            vec![
                header,
                body,
                Node::new(Label::new(
                    "READY    /    Click a control to begin. Animations are paused.",
                    t.muted,
                    11.,
                ))
                .with_id(self.status),
            ],
        ));
        let palette_body = Node::new(Flex::column(14.).padding(24.).lengths([
            Length::Fixed(30.),
            Length::Fixed(48.),
            Length::Fill(1.),
            Length::Fixed(40.),
        ]))
        .children([
            label("Everything is a widget.", 22., t.foreground),
            Node::new(TextInput::new("Try typing, then Tab", t)),
            Node::new(List::new(
                [
                    "Text input",
                    "Reusable composition",
                    "Custom canvas",
                    "Explicit animation",
                ]
                .map(str::to_string),
                t,
            )),
            Node::new(Button::new("Back to the studio", t)).with_id(self.close),
        ]);
        let mut modal = Node::new(Modal::new(Size::new(460., 390.), t))
            .with_id(self.palette)
            .child(palette_body);
        modal.hidden = true;
        Node::new(Stack).children([content, modal])
    }
    fn message(&mut self, message: Message, ui: &mut Ui, now: Duration, text: &mut NativeText) {
        if message.source == self.open && message.value.is::<Clicked>() {
            ui.root.find_mut(self.palette).unwrap().hidden = false;
            ui.invalidate();
            ui.layout(text);
            ui.push_modal(self.palette, now, text);
        } else if (message.source == self.close && message.value.is::<Clicked>())
            || message.value.is::<Dismissed>()
        {
            ui.pop_modal(now, text);
            ui.root.find_mut(self.palette).unwrap().hidden = true;
            ui.invalidate();
        } else if message.source == self.animate && message.value.is::<Clicked>() {
            ui.send(
                self.fire,
                Event::Key {
                    key: Key::Character(' '),
                    modifiers: Modifiers::default(),
                },
                now,
                text,
            );
        } else if message.source == self.note_list {
            if let Some(Activated(i)) = message.value.downcast_ref::<Activated>() {
                let content=match i {0=>INTRO,1=>"Composition\n\nA row is a widget that lays out its children. A panel adds padding and a background. A scroll view changes the child's offset. You can replace any of them.\n\nThe application owns this tree. Widgets own their interaction state.",2=>"Text & selection\n\nTry these:\n\nType ab, press Left, then type X.\n\nSelect a range and replace it. Undo should restore the whole replacement.\n\nCombining characters: cafe\u{301}.\nEmoji sequences: 👩‍💻.\nCyrillic: Привет, мир.\n\nMove between the two fields above. Their carets are independent.",3=>"Frame scheduling\n\nAn idle window waits for the operating system.\n\nA focused caret requests its next blink. A running game requests its next frame. A paused game requests nothing.\n\nNo global polling timer drives this UI.",4=>"Custom drawing\n\nThe renderer accepts shapes, paths, text runs, transforms, clips, and gradients.\n\nThe fire beside this editor is implemented in the application. The core knows nothing about fire.\n\nBackend-specific drawing can use the optional CustomPaint hook.",_=>"Game canvas\n\nClick the game to start. Move the pointer to control the paddle.\n\nThe same event routing, pointer coordinates, drawing, and frame scheduler serve both this editor and the game.\n\nThe game has no separate window or event loop."};
                ui.edit::<TextInput>(self.editor, |editor| editor.set_text(content));
            }
        }
        if message.value.is::<TextChanged>() {
            self.count += 1;
            let count = self.count;
            ui.edit::<Label>(self.status,|label|label.text=format!("EDITING    /    {count} changes in this session. Notes in this studio are temporary."));
        }
    }
}
fn main() {
    if let Err(e) = run(
        Studio::new(),
        WindowOptions {
            title: "Fire UI / Studio".into(),
            retain_surface: std::env::args().any(|a| a == "--retained"),
            ..WindowOptions::default()
        },
    ) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
