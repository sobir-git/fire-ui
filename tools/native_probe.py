#!/usr/bin/env python3
"""Exercise an isolated X11 window and measure its process, including software GL.

Requires Linux, Xvfb, xdotool, and Pillow with XCB support. Never uses the user's
display or notes. Artifacts are screenshots and JSON, not golden-image approvals.
"""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from probe_lifetime import install_signal_cleanup, run_owned

from PIL import ImageGrab
from fire_ui_inspect import request as inspect_request


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/release/fire-ui-studio")
    parser.add_argument("--seconds", type=float, default=3.0)
    parser.add_argument("--output", default="artifacts", help="Directory for this run's measurements and screenshots")
    parser.add_argument("--accessibility", action="store_true", help="Exercise AT-SPI on a private session bus; run under dbus-run-session")
    args = parser.parse_args()
    if args.accessibility and os.environ.get("_FIRE_UI_PROBE_PRIVATE_BUS") != "1":
        private_env = {**os.environ, "_FIRE_UI_PROBE_PRIVATE_BUS": "1", "GSETTINGS_BACKEND": "memory"}
        raise SystemExit(run_owned(["dbus-run-session", "--", sys.executable, *sys.argv], env=private_env).returncode)
    if args.seconds <= 0:
        parser.error("--seconds must be positive")
    binary = Path(args.binary).resolve()
    artifacts = Path(args.output)
    artifacts.mkdir(parents=True, exist_ok=True)
    results = {"binary": str(binary), "binary_bytes": binary.stat().st_size, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
               "backend": "Xvfb / Mesa software GL", "samples": {}}
    ticks = os.sysconf("SC_CLK_TCK")

    with tempfile.TemporaryDirectory(prefix="fire-ui-probe-") as directory:
        with open(Path(directory) / "display", "w+") as display_file:
            display = subprocess.Popen(
                ["Xvfb", "-displayfd", str(display_file.fileno()), "-screen", "0",
                 "1200x900x24", "-nolisten", "tcp"],
                pass_fds=(display_file.fileno(),), stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            app = None
            try:
                number = ""
                # A loaded CI runner can take several seconds to hand back the
                # display number; five was not enough.
                for _ in range(600):
                    display_file.seek(0)
                    number = display_file.read().strip()
                    if number:
                        break
                    if display.poll() is not None:
                        raise RuntimeError(f"Xvfb exited with {display.returncode}")
                    time.sleep(0.05)
                if not number:
                    raise RuntimeError("Xvfb did not start within 30s")
                env = {**os.environ, "DISPLAY": ":" + number,
                       "WINIT_UNIX_BACKEND": "x11", "LIBGL_ALWAYS_SOFTWARE": "1", "FIRE_UI_PROFILE": "1", "FIRE_UI_INSPECT": str(Path(directory) / "ui.sock")}
                env.pop("WAYLAND_DISPLAY", None)
                if args.accessibility:
                    runtime = Path(directory) / "runtime"
                    runtime.mkdir(mode=0o700)
                    env.update(XDG_RUNTIME_DIR=str(runtime), GSETTINGS_BACKEND="memory")
                    subprocess.run(["dbus-update-activation-environment", "DISPLAY", "XDG_RUNTIME_DIR", "GSETTINGS_BACKEND"], env=env, check=True, timeout=5)
                with open(artifacts / "native.log", "w") as log:
                    app = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log)

                    def x(*arguments):
                        return subprocess.check_output(
                            ["xdotool", *map(str, arguments)], env=env,
                            stderr=subprocess.DEVNULL,
                        ).decode().strip()

                    window = None
                    for _ in range(100):
                        if app.poll() is not None:
                            raise RuntimeError((artifacts / "native.log").read_text())
                        try:
                            window = x("search", "--name", "Fire UI Studio").splitlines()[0]
                            break
                        except subprocess.CalledProcessError:
                            time.sleep(0.1)
                    if window is None:
                        raise RuntimeError("Studio window did not appear")
                    x("windowfocus", window)
                    time.sleep(1.0)

                    def state():
                        # Fields after the final ')' begin at proc stat field 3.
                        fields = Path(f"/proc/{app.pid}/stat").read_text().rsplit(")", 1)[1].split()
                        status = dict(line.split(":", 1) for line in
                                      Path(f"/proc/{app.pid}/status").read_text().splitlines())
                        return {"ticks": int(fields[11]) + int(fields[12]),
                                "rss_kib": int(status["VmRSS"].split()[0]),
                                "threads": int(status["Threads"])}

                    def sample(name, seconds=None):
                        before = state()
                        started = time.monotonic()
                        time.sleep(args.seconds if seconds is None else seconds)
                        elapsed = time.monotonic() - started
                        after = state()
                        results["samples"][name] = {
                            "seconds": round(elapsed, 3),
                            "cpu_percent_one_core": round(
                                (after["ticks"] - before["ticks"]) / ticks / elapsed * 100, 3),
                            "cpu_ticks": after["ticks"] - before["ticks"],
                            "rss_kib": after["rss_kib"], "threads": after["threads"],
                        }

                    def shot(name):
                        ImageGrab.grab(xdisplay=env["DISPLAY"]).save(artifacts / name)

                    def click(px, py):
                        x("mousemove", "--window", window, px, py)
                        x("click", 1)


                    sock = Path(directory) / "ui.sock"

                    def snapshot():
                        reply = inspect_request(sock, {})
                        if "error" in reply:
                            raise RuntimeError(reply["error"])
                        return reply["nodes"]

                    def find(**expected):
                        """The one published node matching every field, e.g. role="Button"."""
                        for _ in range(40):
                            found = [n for n in snapshot()
                                     if all(n.get(k) == v for k, v in expected.items())]
                            if len(found) == 1:
                                return found[0]
                            if len(found) > 1:
                                raise RuntimeError(f"Ambiguous target {expected}: {len(found)} nodes")
                            time.sleep(0.05)
                        raise RuntimeError(f"No node matching {expected}")

                    def tap(node):
                        """Click the centre of a node, in real window pixels."""
                        b = node["bounds"]
                        click(round(b["x"] + b["width"] / 2), round(b["y"] + b["height"] / 2))
                        time.sleep(0.12)

                    def logged():
                        return (artifacts / "native.log").read_text()

                    def expect_log(*needles, why=""):
                        for _ in range(60):
                            text = logged()
                            if all(n in text for n in needles):
                                return
                            time.sleep(0.05)
                        raise RuntimeError(f"Missing studio output {needles!r} {why}\n{logged()}")

                    def navigate(section):
                        """Switch pages through the same semantic action assistive
                        technology uses, then wait for the page to publish itself."""
                        tab = find(role="Tab", label=section)
                        reply = inspect_request(sock, {"id": tab["id"], "action": "activate"})
                        if "error" in reply:
                            raise RuntimeError(f"Navigating to {section}: {reply['error']}")
                        for _ in range(60):
                            if find(role="Tab", label=section)["selected"]:
                                time.sleep(0.2)
                                return
                            time.sleep(0.05)
                        raise RuntimeError(f"{section} did not become the selected section")

                    def accessibility_probe(navigate, find):
                        content = "AT-SPI café שלום"
                        # Only the visible page publishes nodes, so the button and the
                        # editor are verified from the page each one lives on.
                        navigate("Text")
                        snapshot = inspect_request(Path(directory) / "ui.sock", {})
                        editor = next(n for n in snapshot["nodes"] if n["text"] and n["text"]["multiline"])
                        reply = inspect_request(Path(directory) / "ui.sock", {"id": editor["id"], "action": "set_value", "value": content})
                        if "error" in reply: raise RuntimeError(reply["error"])
                        def call(*arguments):
                            return subprocess.check_output(["gdbus", "call", *arguments], env=env, stderr=subprocess.PIPE, timeout=5).decode()
                        # The probe's private X display selects a separate AT-SPI bus.
                        call("--session", "--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus", "--method", "org.freedesktop.DBus.Properties.Set", "org.a11y.Status", "IsEnabled", "<true>")
                        address = call("--session", "--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus", "--method", "org.a11y.Bus.GetAddress").split("'")[1]
                        def remote(bus, path, method, *arguments):
                            return call("--address", address, "--dest", bus, "--object-path", path, "--method", method, "--", *arguments)
                        def children(bus, path):
                            result = remote(bus, path, "org.a11y.atspi.Accessible.GetChildren")
                            return re.findall(r"\('([^']+)', (?:objectpath )?'([^']+)'\)", result)
                        applications = []
                        for _ in range(40):
                            applications = children("org.a11y.atspi.Registry", "/org/a11y/atspi/accessible/root")
                            if applications: break
                            time.sleep(0.05)
                        queue = list(applications)
                        visited = set()
                        trace = []
                        button_verified = False
                        text_verified = False
                        restarted = False
                        while queue:
                            bus, path = queue.pop(0)
                            if (bus, path) in visited: continue
                            visited.add((bus, path))
                            name = remote(bus, path, "org.freedesktop.DBus.Properties.Get", "org.a11y.atspi.Accessible", "Name")
                            trace.append({"bus": bus, "path": path, "name": name})
                            if "Primary" in name and not button_verified:
                                interfaces = remote(bus, path, "org.a11y.atspi.Accessible.GetInterfaces")
                                if "org.a11y.atspi.Action" in interfaces:
                                    accepted = remote(bus, path, "org.a11y.atspi.Action.DoAction", "0")
                                    if "true" not in accepted: raise RuntimeError("Accessibility activation was rejected")
                                    for _ in range(20):
                                        delivered = (artifacts / "native.log").read_text()
                                        if "Pressed Primary" in delivered:
                                            button_verified = True
                                            break
                                        time.sleep(0.05)
                                    if not button_verified:
                                        raise RuntimeError("Accessibility action did not reach the button")
                            if not text_verified:
                                interfaces = remote(bus, path, "org.a11y.atspi.Accessible.GetInterfaces")
                                if "org.a11y.atspi.Text'" in interfaces:
                                    for _ in range(30):
                                        value = remote(bus, path, "org.a11y.atspi.Text.GetText", "0", "-1")
                                        if content in value: break
                                        time.sleep(.05)
                                    else:
                                        queue.extend(children(bus, path))
                                        continue
                                    accepted = remote(bus, path, "org.a11y.atspi.Text.SetCaretOffset", "12")
                                    if "true" not in accepted: raise RuntimeError("Accessible caret move was rejected")
                                    for _ in range(30):
                                        caret = remote(bus, path, "org.freedesktop.DBus.Properties.Get", "org.a11y.atspi.Text", "CaretOffset")
                                        if "12" in caret: break
                                        time.sleep(.05)
                                    else: raise RuntimeError("Accessible caret offset did not round-trip")
                                    if "org.a11y.atspi.EditableText" not in interfaces:
                                        raise RuntimeError("Native EditableText interface missing")
                                    updated = "Edited through AT-SPI: é שלום 🔥"
                                    accepted = remote(bus, path, "org.a11y.atspi.EditableText.SetTextContents", updated)
                                    if "true" not in accepted:
                                        raise RuntimeError("Native text replacement rejected")
                                    for _ in range(30):
                                        value = remote(bus, path, "org.a11y.atspi.Text.GetText", "0", "-1")
                                        if updated in value: break
                                        time.sleep(.05)
                                    else:
                                        raise RuntimeError("Native text replacement did not reach the editor")
                                    # Read through the independent inspector and use the normal
                                    # keyboard undo path to establish that editing shares history.
                                    snapshot = inspect_request(Path(directory) / "ui.sock", {})
                                    actual = next(n for n in snapshot["nodes"] if n["id"] == editor["id"])
                                    if actual["value"] != updated:
                                        raise RuntimeError("Native editing and inspector disagree")
                                    inspect_request(Path(directory) / "ui.sock", {"id": editor["id"], "action": "focus"})
                                    x("key", "ctrl+z")
                                    for _ in range(30):
                                        value = remote(bus, path, "org.a11y.atspi.Text.GetText", "0", "-1")
                                        if content in value: break
                                        time.sleep(.05)
                                    else:
                                        raise RuntimeError("Native replacement did not preserve undo")
                                    def editable(method, *args):
                                        return remote(bus, path, "org.a11y.atspi.EditableText." + method, *map(str, args))
                                    def expect_text(expected):
                                        for _ in range(40):
                                            snap = inspect_request(Path(directory) / "ui.sock", {})
                                            current = next(n for n in snap["nodes"] if n["id"] == editor["id"])
                                            native = remote(bus, path, "org.a11y.atspi.Text.GetText", "0", "-1")
                                            if current["value"] == expected and expected in native:
                                                return current
                                            time.sleep(.025)
                                        raise RuntimeError(f"Native edit mismatch: expected {expected!r}, got {current['value']!r}")
                                    editable("SetTextContents", "aé🔥e\u0301!")
                                    expect_text("aé🔥e\u0301!")
                                    # Position counts characters; length counts UTF-8 bytes.
                                    editable("InsertText", 2, "中unused", 3)
                                    expect_text("aé中🔥e\u0301!")
                                    editable("DeleteText", 3, 4)
                                    current = expect_text("aé中e\u0301!")
                                    selection_before = current["text"]
                                    editable("CopyText", 1, 3)
                                    current = expect_text("aé中e\u0301!")
                                    if current["text"] != selection_before:
                                        raise RuntimeError("Native copy changed selection")
                                    editable("PasteText", 6)
                                    expect_text("aé中e\u0301!é中")
                                    editable("CutText", 1, 3)
                                    expect_text("ae\u0301!é中")
                                    editable("PasteText", 0)
                                    expected = "é中ae\u0301!é中"
                                    expect_text(expected)
                                    for method, args in [
                                        ("InsertText", (-1, "x", 1)),
                                        ("InsertText", (0, "é", 1)),
                                        ("DeleteText", (3, 1)),
                                        ("DeleteText", (0, 10000)),
                                        ("PasteText", (-1,)),
                                        ("CopyText", (-1, 1)),
                                    ]:
                                        try:
                                            editable(method, *args)
                                        except subprocess.CalledProcessError:
                                            pass
                                        else:
                                            raise RuntimeError(f"Invalid {method} was accepted")
                                        expect_text(expected)
                                    editable("InsertText", 0, "é", 100)
                                    expect_text("é" + expected)
                                    x("key", "ctrl+z")
                                    expect_text(expected)
                                    # A real X11 owner that ignores conversion requests makes
                                    # clipboard reads slow. The UI must keep serving input, and a
                                    # failed read must not overwrite edits made during the wait.
                                    owner_code = r"""
import ctypes as c
import sys
x = c.CDLL('libX11.so.6')
x.XOpenDisplay.argtypes = [c.c_char_p]; x.XOpenDisplay.restype = c.c_void_p
x.XDefaultRootWindow.argtypes = [c.c_void_p]; x.XDefaultRootWindow.restype = c.c_ulong
x.XCreateSimpleWindow.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_int, c.c_uint, c.c_uint, c.c_uint, c.c_ulong, c.c_ulong]; x.XCreateSimpleWindow.restype = c.c_ulong
x.XInternAtom.argtypes = [c.c_void_p, c.c_char_p, c.c_int]; x.XInternAtom.restype = c.c_ulong
x.XSetSelectionOwner.argtypes = [c.c_void_p, c.c_ulong, c.c_ulong, c.c_ulong]
x.XSync.argtypes = [c.c_void_p, c.c_int]
d = x.XOpenDisplay(None)
w = x.XCreateSimpleWindow(d, x.XDefaultRootWindow(d), 0, 0, 1, 1, 0, 0, 0)
x.XSetSelectionOwner(d, x.XInternAtom(d, b'CLIPBOARD', 0), w, 0)
x.XSync(d, 0)
print('ready', flush=True)
sys.stdin.read()
"""
                                    owner = subprocess.Popen([sys.executable, "-c", owner_code], env=env,
                                                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
                                    pending_paste = None
                                    try:
                                        if owner.stdout.readline().strip() != "ready":
                                            raise RuntimeError("Test clipboard owner failed")
                                        pending_paste = subprocess.Popen(["gdbus", "call", "--address", address,
                                            "--dest", bus, "--object-path", path, "--method",
                                            "org.a11y.atspi.EditableText.PasteText", "0"], env=env,
                                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                                        time.sleep(.2)
                                        if pending_paste.poll() is not None:
                                            raise RuntimeError("Slow clipboard did not hold the paste request")
                                        started = time.monotonic()
                                        reply = inspect_request(Path(directory) / "ui.sock", {
                                            "id": editor["id"], "action": "set_value", "value": "changed during clipboard wait"})
                                        latency = time.monotonic() - started
                                        if "error" in reply or latency > 1:
                                            raise RuntimeError("Clipboard wait blocked UI input")
                                        pending_paste.communicate(timeout=6)
                                        if pending_paste.returncode == 0:
                                            raise RuntimeError("Unresponsive clipboard paste reported success")
                                        expect_text("changed during clipboard wait")
                                        results["clipboard_wait_ui_reply_ms"] = round(latency * 1000, 3)
                                    finally:
                                        if pending_paste is not None and pending_paste.poll() is None:
                                            pending_paste.terminate()
                                            pending_paste.wait(timeout=5)
                                        owner.terminate()
                                        owner.wait(timeout=5)
                                    # Restore the pre-probe text for subsequent checks.
                                    for _ in range(100):
                                        try:
                                            editable("SetTextContents", content)
                                            break
                                        except subprocess.CalledProcessError as error:
                                            # The bounded clipboard worker retains its slot until
                                            # arboard finishes its format fallback timeouts.
                                            if b"LimitsExceeded" not in error.stderr: raise
                                            time.sleep(.1)
                                    else:
                                        raise RuntimeError("Clipboard worker did not release its edit slot")
                                    expect_text(content)
                                    text_verified = True
                            if button_verified and text_verified:
                                return {"nodes_visited": len(visited), "button_activation": "primary button press verified", "text": "Unicode read/caret, all six EditableText methods, byte/scalar offsets, rejected invalid edits and undo verified"}
                            queue.extend(children(bus, path))
                            if not queue and text_verified and not button_verified and not restarted:
                                # The editor and the button live on different pages, and
                                # only the visible page is published. Having finished the
                                # text checks, walk the tree again on the button's page.
                                restarted = True
                                navigate("Controls")
                                queue = list(children("org.a11y.atspi.Registry", "/org/a11y/atspi/accessible/root"))
                                visited = set()
                        (artifacts / "accessibility-tree.json").write_text(json.dumps(trace, indent=2))
                        raise RuntimeError(f"Accessibility incomplete: button={button_verified}, text={text_verified}")

                    sample("idle")
                    shot("studio-overview.png")
                    # Every page is reachable, and each one publishes a heading.
                    sections = [n["label"] for n in snapshot() if n["role"] == "Tab"]
                    if len(sections) != 7:
                        raise RuntimeError(f"Expected seven sections, found {sections}")

                    navigate("Controls")
                    shot("studio-controls.png")
                    for name in ["Primary", "Secondary", "Ghost", "Delete"]:
                        tap(find(role="Button", label=name))
                    expect_log(*[f"Pressed {n}" for n in ["Primary", "Secondary", "Ghost", "Delete"]],
                               why="(button styles)")
                    disabled = find(role="Button", label="Disabled")
                    if not disabled["disabled"] or disabled["actions"]:
                        raise RuntimeError("A disabled button still advertises actions")
                    tap(disabled)
                    if "Pressed Disabled" in logged():
                        raise RuntimeError("A disabled button was activated")

                    box = find(role="CheckBox")
                    if box["checked"]:
                        raise RuntimeError("Checkbox did not start clear")
                    tap(box)
                    expect_log("Checkbox is now ticked")
                    if not find(role="CheckBox")["checked"]:
                        raise RuntimeError("Checkbox state did not reach accessibility")

                    switch = find(role="Switch", label="Animate")
                    tap(switch)
                    expect_log("Animation on")

                    slider = find(role="Slider")
                    if slider["range"]["now"] != 62:
                        raise RuntimeError(f"Slider published {slider['range']} not 62")
                    x("mousemove", "--window", window,
                      round(slider["bounds"]["x"] + slider["bounds"]["width"] / 2),
                      round(slider["bounds"]["y"] + slider["bounds"]["height"] / 2))
                    x("click", 1)
                    time.sleep(0.15)
                    x("key", "Right", "Right")
                    time.sleep(0.2)
                    moved = find(role="Slider")["range"]["now"]
                    if not 45 < moved < 60:
                        raise RuntimeError(f"Slider did not follow pointer and arrows: {moved}")
                    reply = inspect_request(sock, {"id": slider["id"], "action": "set_value", "value": "88"})
                    if "error" in reply:
                        raise RuntimeError(reply["error"])
                    expect_log("Warmth 88", why="(assistive-technology set_value)")
                    reply = inspect_request(sock, {"id": slider["id"], "action": "set_value", "value": "not a number"})
                    if "error" not in reply:
                        raise RuntimeError("Slider accepted a value that is not a number")

                    # The dropdown's list is a modal overlay, positioned in window
                    # coordinates so it escapes the page's viewport.
                    choice = find(role="Menu", label="Palette")
                    tap(choice)
                    paper = find(role="MenuItem", label="Paper")
                    if paper["bounds"]["y"] <= choice["bounds"]["y"]:
                        raise RuntimeError("Dropdown list did not open below its field")
                    tap(paper)
                    expect_log("Chose Paper")
                    if find(role="Menu", label="Palette")["value"] != "Paper":
                        raise RuntimeError("Dropdown did not adopt the chosen option")
                    shot("studio-controls-used.png")
                    sample("controls")

                    navigate("Text")
                    editor = find(role="TextInput", value=None) if False else next(
                        n for n in snapshot() if n["text"] and n["text"]["multiline"])
                    tap({"bounds": editor["bounds"]})
                    x("key", "ctrl+a")
                    x("type", "--clearmodifiers", "ab")
                    x("key", "Left")
                    x("type", "X")
                    time.sleep(0.25)
                    expect_log("3 characters in the note")
                    shot("studio-editing.png")
                    sample("focused_caret")

                    navigate("Lists")
                    search = next(n for n in snapshot()
                                  if n["text"] and not n["text"]["multiline"])
                    tap(search)
                    x("type", "--clearmodifiers", "99999")
                    time.sleep(0.35)
                    # Rows are the caller's own widgets; the list publishes itself but
                    # not per-row list items, so the visible row text is the target.
                    tap(find(role="Text", label="Material study 99999"))
                    expect_log("Selected material study 99999")
                    shot("studio-lists.png")

                    navigate("Canvas")
                    canvas = find(role="Canvas")
                    tap(find(role="Switch", label="Animate"))
                    expect_log("Animating")
                    time.sleep(0.4)
                    sample("animation")
                    shot("studio-animation.png")
                    # Direct pointer response: move, then wait for the paddle pixels.
                    paddle_y = round(canvas["bounds"]["y"] + canvas["bounds"]["height"] - 15)
                    pointer_samples = []
                    left = round(canvas["bounds"]["x"] + canvas["bounds"]["width"] * 0.25)
                    right = round(canvas["bounds"]["x"] + canvas["bounds"]["width"] * 0.75)
                    for target in [left, right] * 10:
                        started = time.monotonic()
                        x("mousemove", "--window", window, target, paddle_y - 40)
                        while True:
                            pixels = ImageGrab.grab(xdisplay=env["DISPLAY"])
                            if pixels.getpixel((target, paddle_y))[:3] == (168, 201, 126):
                                pointer_samples.append((time.monotonic() - started) * 1000)
                                break
                            if time.monotonic() - started > 2:
                                raise RuntimeError("Paddle did not follow the mouse")
                    pointer_samples.sort()
                    results["paddle_move_to_visible_ms"] = {
                        "samples": len(pointer_samples),
                        "p50": round(pointer_samples[len(pointer_samples)//2], 3),
                        "p95": round(pointer_samples[int(len(pointer_samples)*0.95)], 3),
                        "max": round(max(pointer_samples), 3),
                        "note": "Injected mouse command to observed paddle pixels; includes command and screenshot overhead on Xvfb.",
                    }
                    shot("studio-canvas.png")
                    tap(find(role="Switch", label="Animate"))
                    expect_log("Paused")
                    sample("paused_again")

                    navigate("Layout")
                    blocks = lambda: [n["bounds"]["x"] for n in snapshot()
                                      if n["role"] == "Text" and n["label"] == "natural"]
                    start = blocks()
                    justify = find(role="Menu", label="Justify")
                    tap(justify)
                    tap(find(role="MenuItem", label="Center"))
                    time.sleep(0.3)
                    if blocks() <= start:
                        raise RuntimeError("Changing Justify did not move the blocks")
                    shot("studio-layout.png")

                    navigate("Theme")
                    shot("studio-theme-dark.png")
                    tap(find(role="Tab", label="Ember light"))
                    expect_log("theme: Ember light")
                    time.sleep(0.4)
                    shot("studio-theme-light.png")
                    sample("light_theme")
                    # The compact theme changes the scale, not only the palette, so
                    # this also exercises a relayout rather than a repaint.
                    body = find(role="Group", label="Fire UI Studio")["bounds"]
                    before = find(role="Tab", label="Compact slate")["bounds"]["height"]
                    tap(find(role="Tab", label="Compact slate"))
                    expect_log("theme: Compact slate")
                    time.sleep(0.5)
                    after = find(role="Tab", label="Compact slate")["bounds"]["height"]
                    if after >= before:
                        raise RuntimeError(
                            f"Compact theme did not relayout: tab height {before} to {after}"
                        )
                    if find(role="Group", label="Fire UI Studio")["bounds"] != body:
                        raise RuntimeError("The window changed size on a theme swap")
                    shot("studio-theme-compact.png")
                    sample("compact_theme")
                    tap(find(role="Tab", label="Ember dark"))
                    expect_log("theme: Ember dark")
                    time.sleep(0.4)

                    # Resizing repeatedly, then at two widths, exercises the rail
                    # collapsing and the grid reflowing.
                    for step in range(60):
                        width = 940 + abs(30 - step) * 6
                        height = 700 + abs(30 - step) * 3
                        x("windowsize", window, width, height)
                        time.sleep(0.012)
                    x("windowsize", window, 940, 720)
                    time.sleep(0.3)
                    navigate("Overview")
                    shot("studio-resized.png")
                    x("windowsize", window, 640, 780)
                    time.sleep(0.4)
                    shot("studio-narrow.png")
                    x("mousemove", "--window", window, 320, 500)
                    x("click", "--repeat", 15, "--delay", 20, 5)
                    time.sleep(0.3)
                    shot("studio-narrow-scrolled.png")
                    if app.poll() is not None:
                        raise RuntimeError("Studio exited during interaction")
                    x("windowsize", window, 1180, 860)
                    time.sleep(0.4)
                    if args.accessibility:
                        results["accessibility"] = accessibility_probe(navigate, find)
                    results["window_smoke"] = "completed"
            finally:
                if app is not None and app.poll() is None:
                    app.terminate()
                    app.wait(timeout=5)
                display.terminate()
                display.wait(timeout=5)
    resize = sorted(int(line.split("=", 1)[1]) / 1000 for line in
                    (artifacts / "native.log").read_text().splitlines()
                    if line.startswith("resize_frame_us="))
    if resize:
        results["resize_event_to_swap_ms"] = {
            "rendered_samples": len(resize), "p50": resize[len(resize)//2],
            "p95": resize[min(len(resize)-1, int(len(resize)*0.95))], "max": max(resize),
            "note": "Latest resize event received by app to completed swap; software GL. Not compositor presentation latency.",
        }
    payload = json.dumps(results, indent=2)
    (artifacts / "native-metrics.json").write_text(payload + "\n")
    print(payload)


if __name__ == "__main__":
    install_signal_cleanup()
    main()
