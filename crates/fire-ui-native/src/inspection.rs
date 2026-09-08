//! Opt-in local automation. No listener exists unless FIRE_UI_INSPECT is set.
use fire_ui::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub revision: Option<u64>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub anchor: Option<usize>,
    #[serde(default)]
    pub caret: Option<usize>,
    pub start: Option<usize>,
    pub end: Option<usize>,
}
pub(crate) struct Pending {
    pub request: Request,
    pub reply: mpsc::SyncSender<Value>,
    pub deadline: Instant,
    outstanding: Arc<AtomicBool>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.outstanding.store(false, Ordering::Release);
    }
}
pub(crate) fn apply<W: Widget>(ui: &mut Ui<W>, r: &Request) -> Result<(), String> {
    if r.revision.is_some_and(|v| v != ui.semantic_revision()) {
        return Err("stale revision; inspect again".into());
    }
    let Some(action) = &r.action else {
        return Ok(());
    };
    if r.id.is_none() && r.key.is_none() && r.label.is_none() {
        return Err("action requires id, key or label".into());
    }
    let nodes = ui.semantics();
    let mut matches = nodes.iter().filter(|n| {
        r.id.is_none_or(|id| id == n.id)
            && r.key
                .as_ref()
                .is_none_or(|key| n.semantics.key.as_ref() == Some(key))
            && r.label
                .as_ref()
                .is_none_or(|label| &n.semantics.label == label)
    });
    let id = matches.next().ok_or("target unavailable")?.id;
    if matches.next().is_some() {
        return Err("ambiguous target; use id or key".into());
    }
    let value = || r.value.clone().ok_or("action requires value".to_string());
    let action = match action.as_str() {
        "focus" => SemanticAction::Focus,
        "activate" => SemanticAction::Activate,
        "set_value" => SemanticAction::SetValue(value()?),
        "replace_selected_text" => SemanticAction::ReplaceSelectedText(value()?),
        "replace_text" => SemanticAction::ReplaceText {
            start: r.start.ok_or("action requires start")?,
            end: r.end.ok_or("action requires end")?,
            text: value()?,
        },
        "set_selection" => SemanticAction::SetSelection {
            anchor: r.anchor.ok_or("action requires anchor")?,
            caret: r.caret.ok_or("action requires caret")?,
        },
        _ => return Err("unknown action".into()),
    };
    ui.accessibility(id, action).map_err(|e| format!("{e:?}"))
}
pub(crate) fn snapshot<W: Widget>(ui: &Ui<W>) -> Value {
    let nodes: Vec<_> = ui.semantics().into_iter().map(|n| {
        let s = n.semantics;
        let mut actions: Vec<_> = s.actions.iter().map(|a| match a {
            SemanticActionKind::Focus => "focus",
            SemanticActionKind::Activate => "activate",
            SemanticActionKind::SetValue => "set_value",
            SemanticActionKind::ReplaceSelectedText => "replace_selected_text",
            SemanticActionKind::ReplaceText => "replace_text",
            SemanticActionKind::SetSelection => "set_selection",
        }).collect();
        if n.focusable && !actions.contains(&"focus") { actions.push("focus"); }
        if s.disabled { actions.clear(); }
        json!({"id": n.id, "parent": n.parent, "key": s.key, "role": format!("{:?}", s.role),
            "label": s.label, "value": s.value, "disabled": s.disabled, "selected": s.selected,
            "checked": s.checked, "focused": n.focused, "actions": actions,
            "bounds": {"x": n.bounds.x, "y": n.bounds.y, "width": n.bounds.width, "height": n.bounds.height},
            "text": s.text.map(|t| json!({"anchor": t.anchor, "caret": t.caret.byte,
                "affinity": format!("{:?}", t.caret.affinity), "multiline": t.multiline,
                "composition": t.composition.map(|c| json!({"text": c.text, "selection": c.selection}))}))})
    }).collect();
    let work = ui.next_work();
    json!({"protocol": 1, "revision": ui.semantic_revision(), "window_focused": ui.window_focused(), "nodes": nodes,
        "size": {"width": ui.size().width, "height": ui.size().height},
        "pending": work.ready, "paint_pending": work.paint, "animating": work.frame})
}

pub(crate) struct Server {
    path: PathBuf,
    alive: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub(crate) fn start(
        path: PathBuf,
        send: impl Fn(Pending) -> bool + Send + 'static,
    ) -> Result<Self, String> {
        use std::os::unix::{fs::PermissionsExt, net::UnixListener};
        // A private directory prevents access between bind and chmod, and protects socket ownership.
        let parent = path
            .parent()
            .ok_or("inspection socket needs a parent directory")?;
        let metadata = std::fs::metadata(parent).map_err(|e| e.to_string())?;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("inspection socket requires a private directory (mode 0700)".into());
        }
        let listener = UnixListener::bind(&path).map_err(|e| format!("inspection socket: {e}"))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        let alive = Arc::new(AtomicBool::new(true));
        let running = alive.clone();
        let thread = std::thread::spawn(move || {
            let outstanding = Arc::new(AtomicBool::new(false));
            for stream in listener.incoming() {
                if !running.load(Ordering::Acquire) {
                    break;
                }
                let Ok(mut stream) = stream else {
                    break;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut line = String::new();
                let result = BufReader::new((&mut stream).take(1_048_577)).read_line(&mut line);
                let reply = if result.is_err() || line.len() > 1_048_576 || !line.ends_with('\n') {
                    json!({"error": "expected one JSON line, at most 1 MiB"})
                } else {
                    match serde_json::from_str::<Request>(&line) {
                        Err(e) => json!({"error": e.to_string()}),
                        Ok(_) if outstanding.load(Ordering::Acquire) => json!({"error": "UI busy"}),
                        Ok(request) => {
                            outstanding.store(true, Ordering::Release);
                            let (reply, receive) = mpsc::sync_channel(1);
                            if !send(Pending {
                                request,
                                reply,
                                deadline: Instant::now() + Duration::from_secs(2),
                                outstanding: outstanding.clone(),
                            }) {
                                break;
                            }
                            receive
                                .recv_timeout(Duration::from_secs(3))
                                .unwrap_or_else(|_| json!({"error": "UI unavailable"}))
                        }
                    }
                };
                let _ = writeln!(stream, "{reply}");
            }
        });
        Ok(Self {
            path,
            alive,
            thread: Some(thread),
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        let _ = std::os::unix::net::UnixStream::connect(&self.path);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn socket_is_private_bounded_and_cleaned_up() {
        use std::os::unix::{fs::PermissionsExt, net::UnixStream};
        let directory =
            std::env::temp_dir().join(format!("fire-inspect-test-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = directory.join("ui.sock");
        let server = Server::start(path.clone(), |pending| {
            pending.reply.send(json!({"accepted": true})).is_ok()
        })
        .unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let mut stream = UnixStream::connect(&path).unwrap();
        writeln!(stream, "{{}}").unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap(),
            json!({"accepted": true})
        );
        let mut stream = UnixStream::connect(&path).unwrap();
        writeln!(stream, "{{\"unknown\": 1}}").unwrap();
        line.clear();
        BufReader::new(stream).read_line(&mut line).unwrap();
        assert!(serde_json::from_str::<Value>(&line)
            .unwrap()
            .get("error")
            .is_some());
        drop(server);
        assert!(!path.exists());
        std::fs::remove_dir(directory).unwrap();
    }
}
