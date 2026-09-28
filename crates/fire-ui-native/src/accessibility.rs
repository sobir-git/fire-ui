use accesskit::{
    Action, ActionData, ActionRequest, Node, NodeId, Role, TextPosition, TextSelection, TreeId,
    TreeInfo, TreeUpdate,
};
use fire_ui::{SemanticAction, SemanticActionKind, SemanticNode};
use std::collections::{HashMap, HashSet};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) struct AccessibilityTree {
    runs: HashMap<u64, NodeId>,
    next: u64,
}
impl Default for AccessibilityTree {
    fn default() -> Self {
        Self::new()
    }
}
impl AccessibilityTree {
    pub(crate) fn new() -> Self {
        Self {
            runs: HashMap::new(),
            next: 0,
        }
    }
    pub(crate) fn action(
        &self,
        request: ActionRequest,
        source: &[SemanticNode],
    ) -> Option<SemanticAction> {
        if request.target_tree != TreeId::ROOT {
            return None;
        }
        match (request.action, request.data) {
            (Action::Click, _) => Some(SemanticAction::Activate),
            (Action::Focus, _) => Some(SemanticAction::Focus),
            (Action::SetValue, Some(ActionData::Value(value))) => {
                Some(SemanticAction::SetValue(value.into()))
            }
            (Action::ReplaceSelectedText, Some(ActionData::Value(value))) => {
                Some(SemanticAction::ReplaceSelectedText(value.into()))
            }
            (Action::SetTextSelection, Some(ActionData::SetTextSelection(selection))) => {
                let node = source.iter().find(|n| n.id == request.target_node.0)?;
                let paragraph = node.semantics.text.as_ref()?.paragraph.as_ref()?;
                let byte = |position: TextPosition| {
                    let (&owner, _) = self.runs.iter().find(|(_, id)| **id == position.node)?;
                    if owner != node.id {
                        return None;
                    }
                    characters(&paragraph.text)
                        .into_iter()
                        .map(|(byte, _)| byte)
                        .chain(std::iter::once(paragraph.text.len()))
                        .nth(position.character_index)
                };
                Some(SemanticAction::SetSelection {
                    anchor: byte(selection.anchor)?,
                    caret: byte(selection.focus)?,
                })
            }
            _ => None,
        }
    }
    // Bounds are in window coordinates; parents do not translate children.
    pub(crate) fn tree(
        &mut self,
        source: Vec<SemanticNode>,
        title: &str,
        scale: f64,
    ) -> TreeUpdate {
        let ids: HashSet<_> = source.iter().map(|n| n.id).collect();
        let mut children: HashMap<u64, Vec<NodeId>> = HashMap::new();
        let mut focus = NodeId(0);
        for n in &source {
            children
                .entry(n.parent.filter(|id| ids.contains(id)).unwrap_or(0))
                .or_default()
                .push(NodeId(n.id));
            if n.focused {
                focus = NodeId(n.id);
            }
        }
        let mut root = Node::new(Role::Window);
        root.set_label(title);
        root.set_children(children.remove(&0).unwrap_or_default());
        let mut nodes = vec![(NodeId(0), root)];
        let mut live_runs = HashSet::new();
        for n in source {
            let role = match n.semantics.role {
                fire_ui::Role::Generic => Role::GenericContainer,
                fire_ui::Role::Button => Role::Button,
                fire_ui::Role::Text => Role::Label,
                fire_ui::Role::TextInput => {
                    if n.semantics.text.as_ref().is_some_and(|t| t.multiline) {
                        Role::MultilineTextInput
                    } else {
                        Role::TextInput
                    }
                }
                fire_ui::Role::List => Role::ListBox,
                fire_ui::Role::ListItem => Role::ListBoxOption,
                fire_ui::Role::Dialog => Role::Dialog,
                fire_ui::Role::Canvas => Role::Canvas,
                fire_ui::Role::CheckBox => Role::CheckBox,
                fire_ui::Role::Radio => Role::RadioButton,
                fire_ui::Role::Switch => Role::Switch,
                fire_ui::Role::Slider => Role::Slider,
                fire_ui::Role::Progress => Role::ProgressIndicator,
                fire_ui::Role::Menu => Role::Menu,
                fire_ui::Role::MenuItem => Role::MenuItem,
                fire_ui::Role::Tab => Role::Tab,
                fire_ui::Role::TabList => Role::TabList,
                fire_ui::Role::Heading => Role::Heading,
                fire_ui::Role::Link => Role::Link,
                fire_ui::Role::Group => Role::Group,
            };
            let mut node = Node::new(role);
            node.set_label(n.semantics.label);
            if let Some(key) = n.semantics.key {
                node.set_author_id(key);
            }
            let compact_text_input = matches!(role, Role::TextInput | Role::MultilineTextInput)
                && n.semantics
                    .text
                    .as_ref()
                    .is_some_and(|text| text.paragraph.is_some());
            if !compact_text_input {
                if let Some(value) = n.semantics.value {
                    node.set_value(value);
                }
            }
            if let Some(range) = n.semantics.range {
                node.set_numeric_value(range.now as f64);
                node.set_min_numeric_value(range.min as f64);
                node.set_max_numeric_value(range.max as f64);
                node.set_numeric_value_step(range.step as f64);
            }
            if n.semantics.disabled {
                node.set_disabled();
            }
            if matches!(role, Role::TextInput | Role::MultilineTextInput)
                && (n.semantics.disabled
                    || !n.semantics.actions.contains(&SemanticActionKind::SetValue))
            {
                node.set_read_only();
            }
            if matches!(role, Role::Tab | Role::ListBoxOption) || n.semantics.selected {
                node.set_selected(n.semantics.selected);
            }
            if let Some(checked) = n.semantics.checked {
                node.set_toggled(if checked {
                    accesskit::Toggled::True
                } else {
                    accesskit::Toggled::False
                });
            }
            if n.bounds.width <= 0. || n.bounds.height <= 0. {
                node.set_hidden();
            }
            if !n.semantics.disabled {
                if n.focusable {
                    node.add_action(Action::Focus);
                }
                for action in n.semantics.actions {
                    // AccessKit has no atomic range replacement action. The Linux
                    // EditableText adapter sends it directly through the shared UI API.
                    if matches!(
                        action,
                        SemanticActionKind::ReplaceText | SemanticActionKind::ScrollBy
                    ) {
                        continue;
                    }
                    node.add_action(match action {
                        SemanticActionKind::Focus => Action::Focus,
                        SemanticActionKind::Activate => Action::Click,
                        SemanticActionKind::SetValue => Action::SetValue,
                        SemanticActionKind::ReplaceSelectedText => Action::ReplaceSelectedText,
                        SemanticActionKind::SetSelection => Action::SetTextSelection,
                        SemanticActionKind::ReplaceText | SemanticActionKind::ScrollBy => {
                            unreachable!()
                        }
                    });
                }
            }
            node.set_bounds(bounds(n.bounds, scale));
            let mut child_ids = children.remove(&n.id).unwrap_or_default();
            if let Some(text) = n.semantics.text {
                if let Some(p) = text.paragraph {
                    live_runs.insert(n.id);
                    let id = *self.runs.entry(n.id).or_insert_with(|| {
                        self.next += 1;
                        NodeId((1 << 63) | self.next)
                    });
                    let mut lengths = Vec::new();
                    let mut anchor = 0;
                    let mut focus = 0;
                    for (byte, len) in character_iter(&p.text) {
                        if byte < text.anchor {
                            anchor += 1
                        }
                        if byte < text.caret.byte {
                            focus += 1
                        }
                        lengths.push(len);
                    }
                    let mut run = Node::new(Role::TextRun);
                    run.set_value(p.text.as_ref());
                    run.set_character_lengths(lengths);
                    node.set_text_selection(TextSelection {
                        anchor: TextPosition {
                            node: id,
                            character_index: anchor,
                        },
                        focus: TextPosition {
                            node: id,
                            character_index: focus,
                        },
                    });
                    child_ids.push(id);
                    nodes.push((id, run));
                }
            }
            node.set_children(child_ids);
            nodes.push((NodeId(n.id), node));
        }
        self.runs.retain(|key, _| live_runs.contains(key));
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(0))),
            tree_id: TreeId::ROOT,
            focus,
        }
    }
}
fn bounds(r: fire_ui::Rect, scale: f64) -> accesskit::Rect {
    accesskit::Rect::new(
        r.x as f64 * scale,
        r.y as f64 * scale,
        (r.x + r.width) as f64 * scale,
        (r.y + r.height) as f64 * scale,
    )
}

// AccessKit stores character lengths in u8. Pathological clusters longer than 255 bytes
// are described as scalars; the editor still rejects selection inside a grapheme.
fn character_iter(value: &str) -> impl Iterator<Item = (usize, u8)> + '_ {
    value.grapheme_indices(true).flat_map(|(base, g)| {
        let long = g.len() > 255;
        std::iter::once((base, g.len() as u8))
            .take(usize::from(!long))
            .chain(
                g.char_indices()
                    .take(if long { usize::MAX } else { 0 })
                    .map(move |(b, c)| (base + b, c.len_utf8() as u8)),
            )
    })
}
fn characters(value: &str) -> Vec<(usize, u8)> {
    character_iter(value).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fire_ui::*;
    use fire_ui_widgets::Editor;
    fn editor(text: &str, anchor: usize, caret: usize) -> Ui<Editor> {
        let mut ui = Ui::new(
            Editor::new(text).map(|editor| {
                editor.restore(fire_ui_widgets::EditorState {
                    wrap: false,
                    ..Default::default()
                })
            }),
            Size::new(300., 200.),
            Limits::default(),
        )
        .unwrap();
        ui.pump(100, |_| {}, |_| {});
        ui.layout(
            &mut fire_ui_text::Text::new(
                fire_ui_fonts::Fonts::load(&[
                    fire_ui_fonts::system_font().expect("test requires a system font")
                ])
                .unwrap(),
            )
            .unwrap(),
        );
        let id = ui.semantics()[0].id;
        ui.accessibility(id, SemanticAction::SetSelection { anchor, caret })
            .unwrap();
        ui
    }

    #[test]
    fn compact_text_input_uses_run_value_and_preserves_other_value_cases() {
        let ui = editor("alpha café", 0, 0);
        let source = ui.semantics();
        let owner = NodeId(source[0].id);
        let update = AccessibilityTree::new().tree(source.clone(), "Test", 1.);
        let parent = &update.nodes.iter().find(|(id, _)| *id == owner).unwrap().1;
        assert_eq!(parent.value(), None);
        assert_eq!(
            update
                .nodes
                .iter()
                .filter(|(_, n)| n.role() == accesskit::Role::TextRun)
                .map(|(_, n)| n.value().unwrap())
                .collect::<String>(),
            "alpha café"
        );
        let mut no_paragraph = source.clone();
        no_paragraph[0].semantics.text.as_mut().unwrap().paragraph = None;
        let update = AccessibilityTree::new().tree(no_paragraph, "Test", 1.);
        assert_eq!(
            update
                .nodes
                .iter()
                .find(|(id, _)| *id == owner)
                .unwrap()
                .1
                .value(),
            Some("alpha café")
        );
        let mut slider = source.clone();
        slider[0].semantics.role = fire_ui::Role::Slider;
        slider[0].semantics.range = Some(fire_ui::Range {
            now: 4.,
            min: 0.,
            max: 10.,
            step: 1.,
        });
        let update = AccessibilityTree::new().tree(slider, "Test", 1.);
        let parent = &update.nodes.iter().find(|(id, _)| *id == owner).unwrap().1;
        assert_eq!(parent.value(), Some("alpha café"));
        assert_eq!(parent.numeric_value(), Some(4.));
        assert_eq!(parent.min_numeric_value(), Some(0.));
        assert_eq!(parent.max_numeric_value(), Some(10.));
        assert_eq!(parent.numeric_value_step(), Some(1.));
        let mut label = source.clone();
        label[0].semantics.role = fire_ui::Role::Text;
        let update = AccessibilityTree::new().tree(label, "Test", 1.);
        assert_eq!(
            update
                .nodes
                .iter()
                .find(|(id, _)| *id == owner)
                .unwrap()
                .1
                .value(),
            Some("alpha café")
        );
    }

    #[test]
    fn text_tree_round_trips_unicode_selection_and_rejects_cross_editor_positions() {
        let mut ui = Ui::new(
            Editor::new("é\nשלום").map(|editor| editor.label("Body")),
            Size::new(300., 200.),
            Limits::default(),
        )
        .unwrap();
        ui.pump(100, |_| {}, |_| {});
        ui.layout(
            &mut fire_ui_text::Text::new(
                fire_ui_fonts::Fonts::load(&[
                    fire_ui_fonts::system_font().expect("test requires a system font")
                ])
                .unwrap(),
            )
            .unwrap(),
        );
        let id = ui.semantics()[0].id;
        ui.accessibility(
            id,
            SemanticAction::SetSelection {
                anchor: 2,
                caret: 7,
            },
        )
        .unwrap();
        let mut bridge = AccessibilityTree::default();
        let tree = bridge.tree(ui.semantics(), "Test", 2.);
        let editor = &tree.nodes.iter().find(|(n, _)| n.0 == id).unwrap().1;
        assert_eq!(editor.role(), accesskit::Role::MultilineTextInput);
        assert!(editor.supports_action(Action::ReplaceSelectedText));
        let selection = *editor.text_selection().unwrap();
        let request = ActionRequest {
            action: Action::SetTextSelection,
            target_tree: TreeId::ROOT,
            target_node: NodeId(id),
            data: Some(ActionData::SetTextSelection(selection)),
        };
        assert_eq!(
            bridge.action(request.clone(), &ui.semantics()),
            Some(SemanticAction::SetSelection {
                anchor: 2,
                caret: 7
            })
        );
        let mut invalid = request;
        if let Some(ActionData::SetTextSelection(selection)) = &mut invalid.data {
            selection.focus.node = NodeId(42);
        }
        assert_eq!(bridge.action(invalid, &ui.semantics()), None);
        let values: String = tree
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == accesskit::Role::TextRun)
            .map(|(_, n)| {
                let value = n.value().unwrap();
                assert_eq!(
                    n.character_lengths()
                        .iter()
                        .map(|v| *v as usize)
                        .sum::<usize>(),
                    value.len()
                );
                value
            })
            .collect();
        assert_eq!(values, "é\nשלום");
    }
}
