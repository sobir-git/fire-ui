#!/usr/bin/env python3
"""Test native X11 focus with an independent Cairo/Editor consumer.

Run: python3 tools/focus_probe.py --output artifacts/focus
Requires cargo, Xvfb, xdotool, libX11, libXtst and a system font. All X11
operations and SIGSTOP batches target temporary fixtures on a private display.
No Fire Notes checkout, user desktop, or session bus is used. This checks core
focus/input behavior, not WM policy or IME composition. Use --repeat 10 for
repeated input bursts. --target-dir can reuse an existing Cargo build cache.
"""
import argparse
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

from fire_ui_inspect import request
from probe_lifetime import install_signal_cleanup, run_owned

ROOT = Path(__file__).resolve().parents[1]
SOURCE = '''use fire_ui_native::{run, WindowOptions, WindowKind};
use fire_ui_widgets::Editor;
fn main() -> Result<(), String> {
    let fonts = fire_ui_fonts::Fonts::load(&[
        fire_ui_fonts::system_font().ok_or("system font unavailable")?
    ])?;
    run(Editor::new(""), WindowOptions {
        title: "Focus Review".into(),
        kind: if std::env::args().any(|x| x == "popup") {
            WindowKind::Popup
        } else { WindowKind::Normal },
        ..Default::default()
    }, fire_ui_text::Text::new(fonts.clone())?, fire_ui_cairo::Cairo::direct(fonts))
}
'''


def stop(process):
    if process.poll() is None:
        # A failing assertion may leave our own fixture stopped.
        process.send_signal(signal.SIGCONT)
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def wait_for(check, description):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(.02)
    raise RuntimeError("Timed out: " + description)


def build(scratch, output, target):
    project = scratch / "consumer"
    (project / "src").mkdir(parents=True)
    (project / "src/main.rs").write_text(SOURCE)
    manifest = '[package]\nname="fire-ui-focus-probe"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[dependencies]\n'
    for crate in ("fire-ui", "fire-ui-native", "fire-ui-cairo", "fire-ui-fonts", "fire-ui-text", "fire-ui-widgets"):
        features = ', features=["x11", "inspection"]' if crate == "fire-ui-native" else ', features=["x11"]' if crate == "fire-ui-cairo" else ''
        manifest += f'{crate} = {{ path={json.dumps(str(ROOT / "crates" / crate))}{features} }}\n'
    (project / "Cargo.toml").write_text(manifest)
    with (output / "build.log").open("w") as log:
        result = run_owned(["cargo", "build", "--release"], cwd=project,
                           env={**os.environ, "CARGO_TARGET_DIR": str(target)}, stdout=log, stderr=log)
    if result.returncode:
        raise RuntimeError(f"Build failed: {output / 'build.log'}")
    binary = output / "fire-ui-focus-probe"
    shutil.copy2(target / "release/fire-ui-focus-probe", binary)
    shutil.copytree(project, output / "consumer", dirs_exist_ok=True)
    return binary


class X11:
    def __init__(self, display):
        self.lib = C.CDLL("libX11.so.6")
        self.xtest = C.CDLL("libXtst.so.6")
        p, u, i = C.c_void_p, C.c_ulong, C.c_int
        signatures = {
            "XOpenDisplay": ([C.c_char_p], p), "XDefaultRootWindow": ([p], u),
            "XCreateSimpleWindow": ([p, u, i, i, C.c_uint, C.c_uint, C.c_uint, u, u], u),
            "XMapWindow": ([p, u], i), "XSync": ([p, i], i),
            "XSetInputFocus": ([p, u, i, u], i), "XGrabKeyboard": ([p, u, i, i, i, u], i),
            "XUngrabKeyboard": ([p, u], i), "XKeysymToKeycode": ([p, u], C.c_uint),
            "XCloseDisplay": ([p], i),
        }
        for name, (args, result) in signatures.items():
            function = getattr(self.lib, name)
            function.argtypes, function.restype = args, result
        self.xtest.XTestFakeKeyEvent.argtypes = [p, C.c_uint, i, u]
        self.display = self.lib.XOpenDisplay(display.encode())
        assert self.display, "Cannot connect to private X server"
        self.root = self.lib.XDefaultRootWindow(self.display)
        self.other = self.lib.XCreateSimpleWindow(self.display, self.root, 0, 0, 100, 100, 0, 0, 0)
        self.lib.XMapWindow(self.display, self.other)
        self.sync()

    def sync(self):
        self.lib.XSync(self.display, 0)

    def focus(self, window):
        self.lib.XSetInputFocus(self.display, window, 1, 0)

    def grab(self):
        assert self.lib.XGrabKeyboard(self.display, self.root, 0, 1, 1, 0) == 0
        self.sync()

    def ungrab(self):
        self.lib.XUngrabKeyboard(self.display, 0)
        self.sync()

    def key(self, keysym, down=True):
        code = self.lib.XKeysymToKeycode(self.display, keysym)
        assert code
        assert self.xtest.XTestFakeKeyEvent(self.display, code, int(down), 0)

    def tap(self, keysym):
        self.key(keysym)
        self.key(keysym, False)

    def close(self):
        self.lib.XCloseDisplay(self.display)


def case(binary, env, scratch, log, x, name, iteration):
    socket = scratch / f"{name}-{iteration}.sock"
    app = subprocess.Popen([str(binary), "popup" if name.startswith("popup-") else "normal"],
                           env={**env, "FIRE_UI_INSPECT": str(socket)}, stdout=log, stderr=log)
    try:
        wait_for(socket.exists, "inspection socket")
        def window_id():
            result = subprocess.run(["xdotool", "search", "--name", "^Focus Review$"],
                                    env=env, text=True, capture_output=True, timeout=5)
            return int(result.stdout.strip().splitlines()[-1]) if result.returncode == 0 else None
        window = wait_for(window_id, "fixture window")
        x.focus(window)
        x.sync()
        time.sleep(.1)
        snapshot = request(socket, {})
        node = next(n for n in snapshot["nodes"] if n["role"] == "TextInput")
        assert "error" not in request(socket, {"id": node["id"], "action": "focus"})
        if name == "popup-grab-only":
            x.grab()
            time.sleep(.1)
            x.ungrab()
            time.sleep(.15)
            assert app.poll() is None, "grab alone dismissed popup"
            result = {"alive": True}
        elif name.startswith("popup-"):
            if name == "popup-grab-transfer":
                x.grab()
                time.sleep(.1)
            app.send_signal(signal.SIGSTOP)
            x.focus(0 if name == "popup-none-return" else 1 if name == "popup-pointerroot-return" else x.other)
            if name.endswith("return"):
                x.focus(window)
            x.sync()
            app.send_signal(signal.SIGCONT)
            if name == "popup-grab-transfer":
                x.ungrab()
            assert app.wait(timeout=3) == 0, "popup did not exit cleanly"
            result = {"exit": 0}
        else:
            if name == "immediate-gain":
                x.focus(x.other)
                x.sync()
                time.sleep(.1)
                app.send_signal(signal.SIGSTOP)
                x.focus(window)
                x.tap(ord('a'))
            elif name == "queued-key-before-loss":
                app.send_signal(signal.SIGSTOP)
                x.tap(ord('a'))
                x.focus(x.other)
            elif name == "rapid-return-input":
                app.send_signal(signal.SIGSTOP)
                x.focus(x.other)
                x.focus(window)
                x.tap(ord('a'))
            elif name == "held-shift-grab":
                x.key(0xffe1)
                x.sync()
                time.sleep(.1)
                x.grab()
                time.sleep(.1)
                x.ungrab()
                time.sleep(.1)
                x.tap(ord('a'))
                x.key(0xffe1, False)
            x.sync()
            app.send_signal(signal.SIGCONT)
            time.sleep(.25)
            actual = next(n for n in request(socket, {})["nodes"] if n["id"] == node["id"])
            result = {k: actual[k] for k in ("value", "focused")}
            expected = {"value": "A" if name == "held-shift-grab" else "a", "focused": name != "queued-key-before-loss"}
            assert result == expected, (name, result, expected)
        return {"case": name, "iteration": iteration, **result}
    finally:
        stop(app)
        x.ungrab()
        x.key(0xffe1, False)
        x.sync()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("artifacts/focus"))
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--repeat", type=int, default=1)
    args = parser.parse_args()
    if args.repeat < 1:
        parser.error("--repeat must be positive")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    evidence = {"passed": False, "checks": [], "backend": "private Xvfb, Cairo, XTest"}
    try:
        with tempfile.TemporaryDirectory(prefix="fire-ui-focus-") as temporary:
            scratch = Path(temporary)
            binary = build(scratch, output, args.target_dir.resolve() if args.target_dir else scratch / "target")
            evidence["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
            env = dict(os.environ)
            for key in ("DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS", "AT_SPI_BUS_ADDRESS", "IBUS_ADDRESS", "SESSION_MANAGER"):
                env.pop(key, None)
            for key in ("HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR"):
                directory = scratch / key.lower()
                directory.mkdir(mode=0o700)
                env[key] = str(directory)
            env.update(WINIT_UNIX_BACKEND="x11", XMODIFIERS="@im=none", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent/fire-ui-focus")
            with (scratch / "display").open("w+") as number, (output / "native.log").open("w") as log:
                server = subprocess.Popen(["Xvfb", "-displayfd", str(number.fileno()), "-screen", "0", "1200x900x24", "-nolisten", "tcp"],
                                          pass_fds=(number.fileno(),), stdout=log, stderr=log)
                try:
                    def display_number():
                        number.seek(0)
                        return number.read().strip()
                    env["DISPLAY"] = ':' + wait_for(display_number, "private display")
                    x = X11(env["DISPLAY"])
                    try:
                        for iteration in range(args.repeat):
                            for name in ("popup-grab-only", "popup-grab-transfer", "popup-rapid-return", "popup-none-return", "popup-pointerroot-return", "immediate-gain", "queued-key-before-loss", "held-shift-grab", "rapid-return-input"):
                                result = case(binary, env, scratch, log, x, name, iteration)
                                evidence["checks"].append(result)
                                print(json.dumps(result), flush=True)
                    finally:
                        x.close()
                finally:
                    stop(server)
        evidence["passed"] = True
    finally:
        (output / "results.json").write_text(json.dumps(evidence, indent=2) + '\n')


if __name__ == "__main__":
    install_signal_cleanup()
    main()
