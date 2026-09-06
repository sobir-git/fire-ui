use fire_ui::*;
use fire_ui_native::*;
use fire_ui_widgets::*;
use std::time::Duration;

struct Counter {
    label: WidgetId,
    count: usize,
}
impl NativeApp for Counter {
    fn build(&mut self) -> Node {
        let theme = Theme::default();
        Node::new(
            Flex::column(16.)
                .padding(32.)
                .lengths([Length::Fixed(40.), Length::Fixed(44.)]),
        )
        .children([
            Node::new(Label::new("Count: 0", theme.foreground, 24.)).with_id(self.label),
            Node::new(Button::new("Increment", theme)),
        ])
    }
    fn message(&mut self, message: Message, ui: &mut Ui, _: Duration, _: &mut NativeText) {
        if message.value.is::<Clicked>() {
            self.count += 1;
            ui.edit::<Label>(self.label, |label| {
                label.text = format!("Count: {}", self.count)
            });
        }
    }
}
fn main() -> Result<(), String> {
    run(
        Counter {
            label: WidgetId::new(),
            count: 0,
        },
        WindowOptions {
            title: "Fire UI / Counter".into(),
            size: Size::new(360., 190.),
            ..WindowOptions::default()
        },
    )
}
