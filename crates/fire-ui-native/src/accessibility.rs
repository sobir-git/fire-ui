use accesskit::{
    Action, ActionData, ActionRequest, Node, NodeId, Role, TextPosition, TextSelection, TreeId,
    TreeInfo, TreeUpdate,
};
use fire_ui::{SemanticAction, SemanticActionKind, SemanticNode};
use std::collections::{HashMap, HashSet};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub(crate) struct AccessibilityTree {
    runs: HashMap<(u64, usize), NodeId>,
    next: u64,
    previous: HashMap<NodeId, Node>,
}
impl AccessibilityTree {
    pub(crate) fn reset(&mut self) {
        self.previous.clear();
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
                    let (&(owner, index), _) =
                        self.runs.iter().find(|(_, id)| **id == position.node)?;
                    if owner != node.id {
                        return None;
                    }
                    let line = paragraph.lines.get(index)?;
                    let end = paragraph
                        .lines
                        .get(index + 1)
                        .map_or(paragraph.text.len(), |l| l.range.start);
                    characters(&paragraph.text[line.range.start..end])
                        .into_iter()
                        .map(|(b, _)| line.range.start + b)
                        .chain(std::iter::once(end))
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
                fire_ui::Role::Menu => Role::Menu,
                fire_ui::Role::MenuItem => Role::MenuItem,
                fire_ui::Role::Tab => Role::Tab,
            };
            let mut node = Node::new(role);
            node.set_label(n.semantics.label);
            if let Some(key) = n.semantics.key {
                node.set_author_id(key);
            }
            if let Some(value) = n.semantics.value {
                node.set_value(value);
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
                    if action == SemanticActionKind::ReplaceText {
                        continue;
                    }
                    node.add_action(match action {
                        SemanticActionKind::Focus => Action::Focus,
                        SemanticActionKind::Activate => Action::Click,
                        SemanticActionKind::SetValue => Action::SetValue,
                        SemanticActionKind::ReplaceSelectedText => Action::ReplaceSelectedText,
                        SemanticActionKind::SetSelection => Action::SetTextSelection,
                        SemanticActionKind::ReplaceText => unreachable!(),
                    });
                }
            }
            node.set_bounds(bounds(n.bounds, scale));
            let mut child_ids = children.remove(&n.id).unwrap_or_default();
            if let Some(text) = n.semantics.text {
                if let Some(p) = text.paragraph {
                    let mut positions = vec![];
                    for (index, line) in p.lines.iter().enumerate() {
                        let key = (n.id, index);
                        live_runs.insert(key);
                        let id = *self.runs.entry(key).or_insert_with(|| {
                            self.next += 1;
                            NodeId((1 << 63) | self.next)
                        });
                        let end = p
                            .lines
                            .get(index + 1)
                            .map_or(p.text.len(), |l| l.range.start);
                        let value = &p.text[line.range.start..end];
                        let mut run = Node::new(Role::TextRun);
                        run.set_value(value);
                        run.set_character_lengths(
                            characters(value)
                                .iter()
                                .map(|(_, len)| *len)
                                .collect::<Vec<_>>(),
                        );
                        let character_geometry: Vec<_> = characters(value)
                            .iter()
                            .map(|(byte, _)| {
                                let byte = line.range.start + byte;
                                let r = line
                                    .cells
                                    .get(line.cells.partition_point(|c| c.range.end <= byte))
                                    .filter(|c| c.range.contains(&byte))
                                    .map_or(
                                        fire_ui::Rect::new(line.width, line.y, 0., p.line_height),
                                        |c| fire_ui::Rect::new(c.x, line.y, c.width, p.line_height),
                                    );
                                n.transform.rect(fire_ui::Rect::new(
                                    r.x + text.origin.x,
                                    r.y + text.origin.y,
                                    r.width,
                                    r.height,
                                ))
                            })
                            .collect();
                        let run_bounds = n.transform.rect(fire_ui::Rect::new(
                            text.origin.x,
                            text.origin.y + line.y,
                            line.width,
                            p.line_height,
                        ));
                        run.set_text_direction(accesskit::TextDirection::LeftToRight);
                        run.set_character_positions(
                            character_geometry
                                .iter()
                                .map(|r| ((r.x - run_bounds.x) as f64 * scale) as f32)
                                .collect::<Vec<_>>(),
                        );
                        run.set_character_widths(
                            character_geometry
                                .iter()
                                .map(|r| (r.width as f64 * scale) as f32)
                                .collect::<Vec<_>>(),
                        );
                        run.set_word_starts(
                            value
                                .unicode_word_indices()
                                .filter_map(|(b, _)| {
                                    u8::try_from(characters(&value[..b]).len()).ok()
                                })
                                .collect::<Vec<_>>(),
                        );
                        run.set_bounds(bounds(
                            n.transform.rect(fire_ui::Rect::new(
                                text.origin.x,
                                text.origin.y + line.y,
                                line.width,
                                p.line_height,
                            )),
                            scale,
                        ));
                        positions.push((line.range.start, end, id));
                        child_ids.push(id);
                        nodes.push((id, run));
                    }
                    let position = |byte: usize, affinity: fire_ui::Affinity| {
                        let mut i = positions
                            .partition_point(|(start, _, _)| *start <= byte)
                            .saturating_sub(1);
                        if affinity == fire_ui::Affinity::Upstream
                            && i > 0
                            && p.lines[i - 1].range.end == byte
                        {
                            i -= 1;
                        }
                        let (start, end, id) = *positions.get(i)?;
                        if byte > end || !p.text.is_char_boundary(byte) {
                            return None;
                        }
                        Some(TextPosition {
                            node: id,
                            character_index: characters(&p.text[start..byte]).len(),
                        })
                    };
                    if let (Some(anchor), Some(focus)) = (
                        position(text.anchor, fire_ui::Affinity::Downstream),
                        position(text.caret.byte, text.caret.affinity),
                    ) {
                        node.set_text_selection(TextSelection { anchor, focus });
                    }
                }
            }
            node.set_children(child_ids);
            nodes.push((NodeId(n.id), node));
        }
        self.runs.retain(|key, _| live_runs.contains(key));
        let current: HashMap<_, _> = nodes.iter().cloned().collect();
        nodes.retain(|(id, node)| self.previous.get(id) != Some(node));
        self.previous = current;
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
fn characters(value: &str) -> Vec<(usize, u8)> {
    value
        .grapheme_indices(true)
        .flat_map(|(base, grapheme)| {
            if let Ok(len) = u8::try_from(grapheme.len()) {
                vec![(base, len)]
            } else {
                grapheme
                    .char_indices()
                    .map(|(b, c)| (base + b, c.len_utf8() as u8))
                    .collect()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fire_ui::*;
    use fire_ui_widgets::Editor;
    #[test]
    fn text_tree_round_trips_unicode_selection_and_rejects_cross_editor_positions() {
        let mut ui = Ui::new(
            Element::leaf(Editor::new("é\nשלום").label("Body")),
            Size::new(300., 200.),
            Limits::default(),
        )
        .unwrap();
        ui.pump(100, |_| {}, |_| {});
        ui.layout(&mut crate::NativeText::new().unwrap());
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
