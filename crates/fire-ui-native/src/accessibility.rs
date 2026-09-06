use accesskit::{Action, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use fire_ui::{SemanticAction, SemanticNode};
use std::collections::{HashMap, HashSet};

pub(crate) fn action(action: Action) -> Option<SemanticAction> {
    match action {
        Action::Click => Some(SemanticAction::Activate),
        Action::Focus => Some(SemanticAction::Focus),
        _ => None,
    }
}
// Every rectangle is already in window coordinates. Bounds do not translate children.
pub(crate) fn tree(source: Vec<SemanticNode>, title: &str, scale: f64) -> TreeUpdate {
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
    for n in source {
        let role = match n.semantics.role {
            fire_ui::Role::Generic => Role::GenericContainer,
            fire_ui::Role::Button => Role::Button,
            fire_ui::Role::Text => Role::Label,
            fire_ui::Role::TextInput => Role::TextInput,
            fire_ui::Role::List => Role::ListBox,
            fire_ui::Role::ListItem => Role::ListBoxOption,
            fire_ui::Role::Dialog => Role::Dialog,
            fire_ui::Role::Canvas => Role::Canvas,
        };
        let mut node = Node::new(role);
        node.set_label(n.semantics.label);
        if let Some(value) = n.semantics.value {
            node.set_value(value);
        }
        if n.semantics.disabled {
            node.set_disabled();
        }
        if n.semantics.selected {
            node.set_selected(true);
        }
        if n.bounds.width <= 0. || n.bounds.height <= 0. {
            node.set_hidden();
        }
        if n.focusable {
            node.add_action(Action::Focus);
        }
        if role == Role::Button && !n.semantics.disabled {
            node.add_action(Action::Click);
        }
        let r = n.bounds;
        node.set_bounds(accesskit::Rect::new(
            r.x as f64 * scale,
            r.y as f64 * scale,
            (r.x + r.width) as f64 * scale,
            (r.y + r.height) as f64 * scale,
        ));
        node.set_children(children.remove(&n.id).unwrap_or_default());
        nodes.push((NodeId(n.id), node));
    }
    TreeUpdate {
        nodes,
        tree: Some(Tree::new(NodeId(0))),
        tree_id: TreeId::ROOT,
        focus,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use fire_ui::*;
    #[test]
    fn modal_orphans_attach_to_window_and_share_scaled_geometry() {
        let update = tree(
            vec![SemanticNode {
                id: 7,
                parent: Some(3),
                bounds: Rect::new(10., 20., 30., 40.),
                focused: true,
                focusable: true,
                semantics: Semantics {
                    role: fire_ui::Role::Button,
                    label: "Confirm".into(),
                    ..Semantics::default()
                },
            }],
            "Test",
            2.,
        );
        assert_eq!(update.focus, NodeId(7));
        assert_eq!(update.nodes[0].1.children(), &[NodeId(7)]);
        let node = &update.nodes[1].1;
        assert_eq!(
            node.bounds(),
            Some(accesskit::Rect::new(20., 40., 80., 120.))
        );
        assert!(node.supports_action(Action::Click));
        assert_eq!(action(Action::Click), Some(SemanticAction::Activate));
    }
}
