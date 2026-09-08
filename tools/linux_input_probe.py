#!/usr/bin/env python3
"""Exercise installed IBus XIM and Orca on a private X11 desktop.

Requires Xvfb, xdotool, IBus (simple and table/cangjie5 engines), Orca and
Pillow. Orca's real speech generation is checked in its debug log; audio output
is deliberately disabled. No user desktop, settings or notes are touched.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

from PIL import ImageGrab
from fire_ui_inspect import request


def wait_for(check, description, seconds=10):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            value = check()
            if value:
                return value
        except (OSError, subprocess.SubprocessError):
            pass
        time.sleep(.05)
    raise RuntimeError("Timed out: " + description)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/release/fire-ui-studio")
    parser.add_argument("--output", default="artifacts/linux-input")
    args = parser.parse_args()
    if os.environ.get("_FIRE_UI_INPUT_PRIVATE_BUS") != "1":
        env = {**os.environ, "_FIRE_UI_INPUT_PRIVATE_BUS": "1"}
        return subprocess.run(["dbus-run-session", "--", sys.executable, *sys.argv], env=env).returncode
    binary = Path(args.binary).resolve()
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    results = {"binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
               "backend": "Xvfb / real IBus XIM / real Orca AT-SPI, audio synthesis disabled", "passed": False, "checks": {}}
    processes = []
    logs = []
    with tempfile.TemporaryDirectory(prefix="fire-ui-input-") as directory:
        root = Path(directory)
        for name in ("home", "config", "cache", "data", "runtime", "orca"):
            (root / name).mkdir(mode=0o700)
        env = {**os.environ, "HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "config"),
               "XDG_CACHE_HOME": str(root / "cache"), "XDG_DATA_HOME": str(root / "data"),
               "XDG_RUNTIME_DIR": str(root / "runtime"), "GSETTINGS_BACKEND": "memory",
               "WINIT_UNIX_BACKEND": "x11", "LIBGL_ALWAYS_SOFTWARE": "1", "XMODIFIERS": "@im=ibus",
               "GTK_IM_MODULE": "ibus", "QT_IM_MODULE": "ibus", "LC_ALL": "C.UTF-8", "GIO_USE_VFS": "local",
               "FIRE_UI_INSPECT": str(root / "ui.sock")}
        for name in ("WAYLAND_DISPLAY", "IBUS_ADDRESS", "AT_SPI_BUS_ADDRESS", "SESSION_MANAGER", "DESKTOP_AUTOSTART_ID"):
            env.pop(name, None)

        def run(*args):
            return subprocess.check_output(list(map(str, args)), env=env, stderr=subprocess.PIPE, timeout=8).decode().strip()

        def start(name, *args):
            log = open(output / (name + ".log"), "w")
            logs.append(log)
            process = subprocess.Popen(list(map(str, args)), env=env, stdout=log, stderr=log)
            processes.append(process)
            return process

        def snapshot():
            return request(root / "ui.sock", {})

        def node(node_id):
            return next(n for n in snapshot()["nodes"] if n["id"] == node_id)

        def action(node_id, action, **fields):
            result = request(root / "ui.sock", {"id": node_id, "action": action, **fields})
            if "error" in result:
                raise RuntimeError(result["error"])
            return result

        def key(*keys):
            run("xdotool", "key", "--clearmodifiers", *keys)
            time.sleep(.12)

        def atspi_replace_contents(value):
            def call(*args):
                return run("gdbus", "call", *args)
            call("--session", "--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus", "--method",
                 "org.freedesktop.DBus.Properties.Set", "org.a11y.Status", "IsEnabled", "<true>")
            address = call("--session", "--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus",
                           "--method", "org.a11y.Bus.GetAddress").split("'")[1]
            def remote(bus, path, method, *args):
                return call("--address", address, "--dest", bus, "--object-path", path, "--method", method, "--", *args)
            queue = [("org.a11y.atspi.Registry", "/org/a11y/atspi/accessible/root")]
            visited = set()
            while queue:
                bus, path = queue.pop(0)
                if (bus, path) in visited:
                    continue
                visited.add((bus, path))
                interfaces = remote(bus, path, "org.a11y.atspi.Accessible.GetInterfaces")
                if "org.a11y.atspi.EditableText" in interfaces:
                    if "'old'" in remote(bus, path, "org.a11y.atspi.Text.GetText", "0", "-1"):
                        accepted = remote(bus, path, "org.a11y.atspi.EditableText.SetTextContents", value)
                        if "true" not in accepted:
                            raise RuntimeError("AT-SPI replacement rejected")
                        return True
                children = remote(bus, path, "org.a11y.atspi.Accessible.GetChildren")
                queue.extend(re.findall(r"\('([^']+)', (?:objectpath )?'([^']+)'\)", children))
            return False

        try:
            with open(root / "display", "w+") as display_file:
                display = subprocess.Popen(["Xvfb", "-displayfd", str(display_file.fileno()), "-screen", "0", "1200x900x24", "-nolisten", "tcp"],
                                           pass_fds=(display_file.fileno(),), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                processes.append(display)
                def display_number():
                    display_file.seek(0)
                    return display_file.read().strip()
                env["DISPLAY"] = ":" + wait_for(display_number, "Xvfb display")
            run("dbus-update-activation-environment", "DISPLAY", "XDG_RUNTIME_DIR", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "HOME", "GSETTINGS_BACKEND")
            results["versions"] = {"ibus": run("ibus", "version"), "orca": run("orca", "--version")}
            start("ibus", "ibus-daemon", "--config=disable", "--emoji-extension=disable", "--cache=none")
            wait_for(lambda: run("ibus", "address"), "IBus bus")
            wait_for(lambda: run("ibus", "engine", "xkb:us::eng") or True, "IBus engine")
            # IBus daemon redirects child errors to /dev/null. Own the XIM process
            # explicitly so startup failures remain visible on minimal CI desktops.
            xim_binary = next((Path(path) for path in (
                "/usr/libexec/ibus-x11", "/usr/lib/ibus/ibus-x11", "/usr/lib64/ibus/ibus-x11"
            ) if Path(path).is_file()), None)
            if xim_binary is None:
                raise RuntimeError("Install the IBus X11 frontend (ibus-x11)")
            xim = start("ibus-xim", xim_binary, "--kill-daemon")
            def xim_ready():
                if xim.poll() is not None:
                    raise RuntimeError("IBus XIM exited: " + (output / "ibus-xim.log").read_text())
                return "ibus" in run("xprop", "-root", "XIM_SERVERS")
            wait_for(xim_ready, "IBus XIM registration")
            start("studio", binary)
            window = wait_for(lambda: run("xdotool", "search", "--name", "Fire UI Studio"), "Studio window").splitlines()[0]
            run("xdotool", "windowfocus", window)
            def ready_snapshot():
                value = snapshot()
                return value if any(n.get("text") for n in value.get("nodes", [])) else None
            initial = wait_for(ready_snapshot, "mounted editor semantics")
            (output / "initial.json").write_text(json.dumps(initial, indent=2))
            editor = next(n for n in initial["nodes"] if n["text"] and n["text"]["multiline"])["id"]
            other = next(n for n in initial["nodes"] if n["text"] and not n["text"]["multiline"])["id"]
            action(editor, "set_value", value="")
            action(editor, "focus")
            run("ibus", "engine", "table:cangjie5")
            time.sleep(.5)
            key("a")
            composition = wait_for(lambda: node(editor)["text"]["composition"], "Cangjie preedit")
            assert composition["text"] == "日" and node(editor)["value"] == "", "Preedit mutated document"
            (output / "ime-preedit.json").write_text(json.dumps(snapshot(), indent=2))
            time.sleep(.3)
            windows = run("xwininfo", "-root", "-tree")
            (output / "ime-windows.txt").write_text(windows)
            candidates = [tuple(map(int, match)) for match in re.findall(r'"ibus-ui-gtk3":.*?\s(\d+)x(\d+)\+(-?\d+)\+(-?\d+)', windows)]
            if not candidates:
                raise RuntimeError("Installed IBus candidate panel did not appear")
            width, height, x, y = max(candidates, key=lambda rectangle: rectangle[0] * rectangle[1])
            results["candidate_panel"] = {"x": x, "y": y, "width": width, "height": height}
            ImageGrab.grab(xdisplay=env["DISPLAY"]).save(output / "ime-preedit.png")
            key("space")
            wait_for(lambda: node(editor)["value"] == "日", "Cangjie candidate commit")
            key("b")
            wait_for(lambda: node(editor)["text"]["composition"], "cancel preedit")
            key("Escape")
            wait_for(lambda: not node(editor)["text"]["composition"], "cancel clears preedit")
            assert node(editor)["value"] == "日", "IME cancel changed document"
            results["checks"]["ibus_cangjie"] = "installed Cangjie 5 XIM composition, candidate commit and cancellation"
            action(editor, "set_selection", anchor=0, caret=3)
            key("b")
            wait_for(lambda: node(editor)["text"]["composition"], "selected replacement preedit")
            assert node(editor)["value"] == "日", "Preedit replaced selection before commit"
            key("space")
            wait_for(lambda: node(editor)["value"] == "月", "IME commit replaces selection")
            results["checks"]["ibus_selection"] = "composition replaces selected Unicode text exactly once at commit"
            key("a")
            wait_for(lambda: node(editor)["text"]["composition"], "native focus-loss preedit")
            focus_window = start("focus-window", "xmessage", "-title", "Probe focus target", "Isolated focus target")
            focus_id = wait_for(lambda: run("xdotool", "search", "--name", "Probe focus target"), "second native window").splitlines()[0]
            run("xdotool", "windowfocus", focus_id)
            wait_for(lambda: not snapshot()["window_focused"], "native focus loss")
            run("xdotool", "windowfocus", window)
            wait_for(lambda: snapshot()["window_focused"], "native focus return")
            key("Escape")
            wait_for(lambda: not node(editor)["text"]["composition"], "composition cancellation after native focus return")
            focus_window.terminate()
            focus_window.wait(timeout=5)
            results["checks"]["ibus_window_focus"] = "native focus loss/return preserves a cancellable IME session"
            key("a")
            wait_for(lambda: node(editor)["text"]["composition"], "focus-change preedit")
            action(other, "focus")
            wait_for(lambda: not node(editor)["text"]["composition"], "focus change clears old composition")
            run("ibus", "engine", "xkb:us::eng")
            run("xdotool", "type", "target")
            wait_for(lambda: "target" in node(other)["value"], "typing after IME focus transfer")
            results["checks"]["ibus_focus"] = "composition teardown and input to newly focused editor"
            for edit_action in ("set_value", "replace_selected_text", "atspi_set_text_contents"):
                action(editor, "focus")
                action(editor, "set_value", value="old")
                action(editor, "set_selection", anchor=0, caret=3)
                run("ibus", "engine", "table:cangjie5")
                key("a")
                wait_for(lambda: node(editor)["text"]["composition"], edit_action + " preedit")
                if edit_action == "atspi_set_text_contents":
                    wait_for(lambda: atspi_replace_contents("replacement"), "native AT-SPI replacement")
                    wait_for(lambda: node(editor)["value"] == "replacement", "native replacement delivery")
                else:
                    action(editor, edit_action, value="replacement")
                after_edit = node(editor)
                key("space")
                after_commit = node(editor)
                results["checks"]["ibus_external_" + edit_action] = {
                    "after_edit": after_edit["value"], "composition": after_edit["text"]["composition"],
                    "after_commit_key": after_commit["value"],
                }
                if after_edit["text"]["composition"] or after_commit["value"] != "replacement ":
                    raise RuntimeError("External " + edit_action + " leaked or retained stale native IME composition")
                run("ibus", "engine", "xkb:us::eng")
                run("xdotool", "type", "x")
                wait_for(lambda: node(editor)["value"] == "replacement x", "typing after " + edit_action)

            # Real Orca dispatches native AT-SPI events and generates utterances.
            # An empty factory list keeps synthesis/audio outside this probe.
            settings = {"general": {"speechServerFactory": "fire_ui_probe_no_audio", "speechFactoryModules": [], "enableSpeech": True, "enableKeyEcho": False, "enableEchoByCharacter": True},
                        "profiles": {"default": {"profile": ["Default", "default"]}}, "pronunciations": {}, "keybindings": {}}
            (root / "orca" / "user-settings.conf").write_text(json.dumps(settings))
            (root / "orca" / "orca-customizations.py").write_text("from orca import debug\nif debug.debugFile:\n    debug.debugFile.reconfigure(line_buffering=True)\n")
            start("orca", "orca", "--user-prefs", root / "orca", "--debug-file", output / "orca-debug.log", "--disable=braille")
            debug_log = output / "orca-debug.log"
            wait_for(lambda: debug_log.exists() and "Starting Atspi main event loop" in debug_log.read_text(), "Orca AT-SPI loop")
            def speech_since(offset):
                return [line for line in debug_log.read_text()[offset:].splitlines() if "SPEECH OUTPUT:" in line]

            def spoken(name, operation, expected):
                offset = len(debug_log.read_text())
                operation()
                def matching_speech():
                    lines = speech_since(offset)
                    return lines if all(any(phrase in line for line in lines) for phrase in expected) else None
                results["checks"]["orca_" + name] = wait_for(matching_speech, "Orca " + name)

            action(editor, "set_value", value="Orca verification")
            action(other, "focus")
            time.sleep(.5)
            spoken("editor_focus", lambda: action(editor, "focus"), ["entry Orca verification"])
            key("Home")
            spoken("caret_navigation", lambda: key("Right"), ["'r'"])
            key("End")
            spoken("insertion", lambda: run("xdotool", "type", "z"), ["'z'"])
            spoken("deletion", lambda: key("BackSpace"), ["'z'"])
            spoken("selection", lambda: key("shift+Left"), ["selected"])
            button = next(n["id"] for n in snapshot()["nodes"] if n["role"] == "Button" and n["label"] == "Add one")
            spoken("button_focus", lambda: action(button, "focus"), ["Add one", "button"])
            spoken("tab_navigation", lambda: key("Tab"), ["Add one", "button"])
            focused = next(n for n in snapshot()["nodes"] if n["focused"])
            assert focused["id"] != button and focused["role"] == "Button", "Tab did not move native keyboard focus"
            key("Return")
            wait_for(lambda: "Right counter: 1" in (output / "studio.log").read_text(), "native activation while Orca runs")
            results["checks"]["orca_button_activation"] = "Tab to second counter and Enter increments it"
            speech = [line for line in debug_log.read_text().splitlines() if "SPEECH OUTPUT:" in line]
            (output / "orca-speech.log").write_text("\n".join(speech) + "\n")
            results["passed"] = True
        finally:
            for process in reversed(processes):
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)
            for log in logs:
                log.close()
            (output / "results.json").write_text(json.dumps(results, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(results, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
