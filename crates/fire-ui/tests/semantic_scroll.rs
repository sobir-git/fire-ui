use fire_ui::*;
use std::convert::Infallible;

struct CustomViewport {
    disabled: bool,
    supported: bool,
    deltas: Vec<Point>,
}
impl Widget for CustomViewport {
    type Command = ();
    type Output = Infallible;
    fn semantics(&self) -> Semantics {
        Semantics {
            role: Role::Group,
            disabled: self.disabled,
            actions: if self.supported {
                vec![SemanticActionKind::ScrollBy]
            } else {
                vec![]
            },
            ..Semantics::default()
        }
    }
    fn accessibility(
        &mut self,
        _: &mut Update<'_, Self>,
        action: SemanticAction,
    ) -> Result<(), SemanticError> {
        if let SemanticAction::ScrollBy(delta) = action {
            self.deltas.push(delta);
            Ok(())
        } else {
            Err(SemanticError::Unsupported)
        }
    }
}
fn viewport(disabled: bool, supported: bool) -> (Ui<CustomViewport>, u64) {
    let mut ui = Ui::new(
        Element::leaf(CustomViewport {
            disabled,
            supported,
            deltas: vec![],
        }),
        Size::new(100., 100.),
        Limits::default(),
    )
    .unwrap();
    ui.pump(100, |_| {}, |_| {});
    ui.layout(&mut TestText);
    let id = ui.semantics()[0].id;
    (ui, id)
}
#[test]
fn custom_widget_receives_scroll_without_editor_or_text_semantics() {
    let (mut ui, id) = viewport(false, true);
    let delta = Point::new(-12., 34.);
    ui.accessibility(id, SemanticAction::ScrollBy(delta))
        .unwrap();
    assert_eq!(ui.root().deltas, vec![delta]);
    assert!(ui.semantics()[0].semantics.text.is_none());
    for invalid in [Point::new(f32::NAN, 0.), Point::new(0., f32::INFINITY)] {
        assert_eq!(
            ui.accessibility(id, SemanticAction::ScrollBy(invalid)),
            Err(SemanticError::InvalidValue)
        );
    }
    assert_eq!(
        ui.root().deltas,
        vec![delta],
        "invalid values must not reach custom code"
    );
}
#[test]
fn unavailable_or_unsupported_scroll_never_invokes_custom_widget() {
    for (disabled, supported, error) in [
        (true, true, SemanticError::Unavailable),
        (false, false, SemanticError::Unsupported),
    ] {
        let (mut ui, id) = viewport(disabled, supported);
        assert_eq!(
            ui.accessibility(id, SemanticAction::ScrollBy(Point::new(1., 2.))),
            Err(error)
        );
        assert!(ui.root().deltas.is_empty());
    }
}
