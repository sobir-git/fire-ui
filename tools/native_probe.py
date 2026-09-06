#!/usr/bin/env python3
"""Exercise an isolated X11 window and measure its process, including software GL.

Requires Linux, Xvfb, xdotool, and Pillow with XCB support. Never uses the user's
display or notes. Artifacts are screenshots and JSON, not golden-image approvals.
"""
import argparse
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from PIL import ImageGrab


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/release/fire-ui-studio")
    parser.add_argument("--seconds", type=float, default=3.0)
    parser.add_argument("--output", default="artifacts", help="Directory for this run's measurements and screenshots")
    parser.add_argument("--accessibility", action="store_true", help="Exercise AT-SPI on a private session bus; run under dbus-run-session")
    args = parser.parse_args()
    if args.accessibility and os.environ.get("_FIRE_UI_PROBE_PRIVATE_BUS") != "1":
        private_env = {**os.environ, "_FIRE_UI_PROBE_PRIVATE_BUS": "1", "GSETTINGS_BACKEND": "memory"}
        raise SystemExit(subprocess.run(["dbus-run-session", "--", sys.executable, *sys.argv], env=private_env).returncode)
    if args.seconds <= 0:
        parser.error("--seconds must be positive")
    binary = Path(args.binary).resolve()
    artifacts = Path(args.output)
    artifacts.mkdir(parents=True, exist_ok=True)
    results = {"binary": str(binary), "binary_bytes": binary.stat().st_size,
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
                for _ in range(100):
                    display_file.seek(0)
                    number = display_file.read().strip()
                    if number:
                        break
                    time.sleep(0.05)
                if not number:
                    raise RuntimeError("Xvfb did not start")
                env = {**os.environ, "DISPLAY": ":" + number,
                       "WINIT_UNIX_BACKEND": "x11", "LIBGL_ALWAYS_SOFTWARE": "1", "FIRE_UI_PROFILE": "1"}
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

                    def accessibility_probe():
                        def call(*arguments):
                            return subprocess.check_output(["gdbus", "call", *arguments], env=env, stderr=subprocess.PIPE, timeout=5).decode()
                        # The probe's private X display selects a separate AT-SPI bus.
                        call("--session", "--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus", "--method", "org.freedesktop.DBus.Properties.Set", "org.a11y.Status", "IsEnabled", "<true>")
                        address = call("--session", "--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus", "--method", "org.a11y.Bus.GetAddress").split("'")[1]
                        def remote(bus, path, method, *arguments):
                            return call("--address", address, "--dest", bus, "--object-path", path, "--method", method, *arguments)
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
                        while queue:
                            bus, path = queue.pop(0)
                            if (bus, path) in visited: continue
                            visited.add((bus, path))
                            name = remote(bus, path, "org.freedesktop.DBus.Properties.Get", "org.a11y.atspi.Accessible", "Name")
                            trace.append({"bus": bus, "path": path, "name": name})
                            if "Add one" in name:
                                interfaces = remote(bus, path, "org.a11y.atspi.Accessible.GetInterfaces")
                                if "org.a11y.atspi.Action" in interfaces:
                                    accepted = remote(bus, path, "org.a11y.atspi.Action.DoAction", "0")
                                    if "true" not in accepted: raise RuntimeError("Accessibility activation was rejected")
                                    for _ in range(20):
                                        delivered = (artifacts / "native.log").read_text()
                                        if "Left counter: 2" in delivered or "Right counter: 2" in delivered:
                                            return {"nodes_visited": len(visited), "button_activation": "counter increment verified"}
                                        time.sleep(0.05)
                                    raise RuntimeError("Accessibility action did not reach the counter")
                            queue.extend(children(bus, path))
                        (artifacts / "accessibility-tree.json").write_text(json.dumps(trace, indent=2))
                        raise RuntimeError("Accessible button was not published")

                    sample("idle")
                    shot("studio-initial.png")
                    click(700, 332)
                    click(950, 332)
                    click(100, 285)
                    x("key", "ctrl+a")
                    x("type", "--clearmodifiers", "ab")
                    x("key", "Left")
                    x("type", "X")
                    time.sleep(0.2)
                    shot("studio-editing.png")
                    sample("focused_caret")
                    log.flush()
                    output = (artifacts / "native.log").read_text()
                    for expected in ["Left counter: 1", "Right counter: 1", "3 characters in your note"]:
                        if expected not in output:
                            raise RuntimeError("Missing interaction result: " + expected + "\n" + output)
                    click(710, 526)
                    x("type", "--clearmodifiers", "99999")
                    time.sleep(0.2)
                    click(710, 570)
                    time.sleep(0.1)
                    if "Selected material study 99999" not in (artifacts / "native.log").read_text():
                        raise RuntimeError("Picker did not filter and emit the selected result")
                    click(710, 526)
                    x("key", "ctrl+a", "BackSpace")
                    click(500, 605)
                    time.sleep(0.2)
                    sample("animation")
                    shot("studio-animation.png")
                    click(500, 605)
                    time.sleep(0.2)
                    sample("paused_again")
                    # Includes injection and screenshot overhead, not physical display latency.
                    pointer_samples = []
                    for target in [160, 470] * 10:
                        started = time.monotonic()
                        x("mousemove", "--window", window, target, 690)
                        while True:
                            pixels = ImageGrab.grab(xdisplay=env["DISPLAY"])
                            if pixels.getpixel((target, 739))[:3] == (196, 217, 169):
                                pointer_samples.append((time.monotonic() - started) * 1000)
                                break
                            if time.monotonic() - started > 2:
                                raise RuntimeError("Paddle did not follow the mouse to the expected position")
                    pointer_samples.sort()
                    results["paddle_move_to_visible_ms"] = {
                        "samples": len(pointer_samples),
                        "p50": round(pointer_samples[len(pointer_samples)//2], 3),
                        "p95": round(pointer_samples[int(len(pointer_samples)*0.95)], 3),
                        "max": round(max(pointer_samples), 3),
                        "note": "Injected mouse command to observed paddle pixels; includes command and screenshot overhead on Xvfb.",
                    }
                    shot("studio-game.png")
                    for step in range(60):
                        width = 940 + abs(30-step)*6
                        height = 700 + abs(30-step)*3
                        x("windowsize", window, width, height)
                        time.sleep(0.012)
                    x("windowsize", window, 940, 720)
                    time.sleep(0.3)
                    shot("studio-resized.png")
                    x("windowsize", window, 640, 780)
                    time.sleep(0.3)
                    shot("studio-narrow.png")
                    x("mousemove", "--window", window, 620, 700)
                    x("click", "--repeat", 15, "--delay", 20, 5)
                    time.sleep(0.3)
                    shot("studio-narrow-scrolled.png")
                    if app.poll() is not None:
                        raise RuntimeError("Studio exited during interaction")
                    if args.accessibility:
                        x("windowsize", window, 1140, 880)
                        time.sleep(0.3)
                        results["accessibility"] = accessibility_probe()
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
    main()
