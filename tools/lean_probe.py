#!/usr/bin/env python3
"""Compare the same draw-only app with minimal and optional native dependencies.

Each variant is an independent consumer workspace and has its own Cargo target
directory: studio features cannot contaminate the minimal measurement. Re-running
uses those caches and records that fact. Runtime samples require Linux, Xvfb,
xdotool and dbus-run-session; Pillow enables screenshots. No user display is used.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]
FEATURES = {
    "minimal": ["x11"],
    "extras": ["x11", "accessibility", "inspection", "clipboard", "dialogs", "bitmap-fonts"],
}


def output(command, **kwargs):
    return subprocess.check_output(command, text=True, **kwargs).strip()


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def build(variant, directory, host):
    project = directory / "consumer"
    (project / "src").mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT / "crates/fire-ui-native/examples/minimal.rs", project / "src/main.rs")
    # JSON string escaping is also valid for these TOML basic strings.
    (project / "Cargo.toml").write_text(
        '[package]\nname = "fire-ui-lean-probe"\nversion = "0.0.0"\nedition = "2021"\n'
        '[workspace]\n[dependencies]\n'
        f'fire-ui = {{ path = {json.dumps(str(ROOT / "crates/fire-ui"))} }}\n'
        f'fire-ui-native = {{ path = {json.dumps(str(ROOT / "crates/fire-ui-native"))}, '
        f'default-features = false, features = {json.dumps(FEATURES[variant])} }}\n'
        '[profile.release]\nopt-level = 3\nlto = "thin"\ncodegen-units = 1\nstrip = true\n'
    )
    target = directory / "target"
    had_cache = target.exists()
    env = {**os.environ, "CARGO_TARGET_DIR": str(target)}
    started = time.monotonic()
    with (directory / "build.log").open("w") as log:
        result = subprocess.run(["cargo", "build", "--release", "--target", host],
                                cwd=project, env=env, stdout=log, stderr=log)
    elapsed = time.monotonic() - started
    if result.returncode:
        raise RuntimeError(f"{variant} build failed: see {directory / 'build.log'}")
    tree = output(["cargo", "tree", "--locked", "--target", host, "--edges", "normal,build",
                   "--prefix", "none", "--format", "{p}"], cwd=project, env=env)
    (directory / "dependencies.txt").write_text(tree + "\n")
    packages = sorted({line.removesuffix(" (*)") for line in tree.splitlines()})
    names = {line.split()[0] for line in packages}
    if variant == "minimal":
        unwanted = sorted(name for name in names if name.startswith(("accesskit", "wayland", "serde"))
                          or name in {"arboard", "image", "png", "native-dialog", "zbus", "atspi"})
        if unwanted:
            raise RuntimeError(f"Optional dependencies leaked into minimal: {unwanted}")
    binary = target / host / "release/fire-ui-lean-probe"
    return binary, {
        "features": FEATURES[variant], "binary": str(binary),
        "binary_bytes": binary.stat().st_size,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "dependency_packages": len(packages) - 1, "dependency_kind": "normal and build, host target",
        "build_seconds": round(elapsed, 3), "target_directory_existed": had_cache,
        "build_cache": "variant-specific target; Cargo source/download cache shared",
        "target_bytes": sum(path.stat().st_size for path in target.rglob("*") if path.is_file()),
    }


def process_state(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    status = dict(line.split(":", 1) for line in Path(f"/proc/{pid}/status").read_text().splitlines())
    rollup = dict(line.split(":", 1) for line in
                  Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines()[1:])
    return {"ticks": int(fields[11]) + int(fields[12]),
            "rss_kib": int(status["VmRSS"].split()[0]),
            "pss_kib": int(rollup["Pss"].split()[0]), "threads": int(status["Threads"])}


def sample(binary, directory, seconds):
    with tempfile.TemporaryDirectory(prefix="fire-ui-lean-") as temporary:
        temporary = Path(temporary)
        runtime = temporary / "runtime"
        runtime.mkdir(mode=0o700)
        with (temporary / "display").open("w+") as display_file:
            display = subprocess.Popen(["Xvfb", "-displayfd", str(display_file.fileno()),
                                        "-screen", "0", "640x480x24", "-nolisten", "tcp"],
                                       pass_fds=(display_file.fileno(),),
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            app = None
            try:
                for _ in range(100):
                    display_file.seek(0)
                    number = display_file.read().strip()
                    if number:
                        break
                    time.sleep(0.05)
                else:
                    raise RuntimeError("Xvfb did not start")
                env = {**os.environ, "DISPLAY": ":" + number, "HOME": str(temporary),
                       "XDG_RUNTIME_DIR": str(runtime), "XDG_CONFIG_HOME": str(temporary / "config"),
                       "XDG_DATA_HOME": str(temporary / "data"), "XDG_CACHE_HOME": str(temporary / "cache"),
                       "WINIT_UNIX_BACKEND": "x11", "WINIT_X11_SCALE_FACTOR": "1",
                       "LIBGL_ALWAYS_SOFTWARE": "1",
                       "GSETTINGS_BACKEND": "memory"}
                for name in ("WAYLAND_DISPLAY", "FIRE_UI_FONT", "FIRE_UI_INSPECT", "FIRE_UI_PROFILE"):
                    env.pop(name, None)
                subprocess.run(["dbus-update-activation-environment", "DISPLAY", "HOME", "XDG_RUNTIME_DIR",
                                "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "GSETTINGS_BACKEND"],
                               env=env, check=True, timeout=5)
                with (directory / "native.log").open("w") as log:
                    app = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log)
                    for _ in range(100):
                        if app.poll() is not None:
                            raise RuntimeError(f"App exited: see {directory / 'native.log'}")
                        found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Fire UI Minimal$"],
                                               env=env, capture_output=True, text=True)
                        if found.returncode == 0:
                            break
                        time.sleep(0.05)
                    else:
                        raise RuntimeError("Minimal window did not appear")
                    time.sleep(1.0)
                    before = process_state(app.pid)
                    started = time.monotonic()
                    time.sleep(seconds)
                    elapsed = time.monotonic() - started
                    after = process_state(app.pid)
                    result = {**after, "seconds": round(elapsed, 3),
                              "cpu_ticks": after["ticks"] - before["ticks"],
                              "cpu_percent_one_core": round((after["ticks"] - before["ticks"]) /
                                                            os.sysconf("SC_CLK_TCK") / elapsed * 100, 3)}
                    del result["ticks"]
                    try:
                        from PIL import ImageGrab
                    except ImportError:
                        result["screenshot"] = "Pillow unavailable"
                    else:
                        screenshot = ImageGrab.grab(xdisplay=env["DISPLAY"]).convert("RGB")
                        screenshot.save(directory / "window.png")
                        for point, expected in [((0, 0), (24, 24, 24)), ((320, 240), (255, 107, 24))]:
                            if any(abs(a - b) > 2 for a, b in zip(screenshot.getpixel(point), expected)):
                                raise RuntimeError(f"Canvas did not render correctly: see {directory / 'window.png'}")
                        result["screenshot"] = str(directory / "window.png")
                        window = found.stdout.splitlines()[0]
                        subprocess.run(["xdotool", "windowsize", window, "480", "360"],
                                       env=env, check=True, timeout=5)
                        time.sleep(0.5)
                        geometry = dict(line.split("=", 1) for line in output(
                            ["xdotool", "getwindowgeometry", "--shell", window], env=env).splitlines())
                        width, height = int(geometry["WIDTH"]), int(geometry["HEIGHT"])
                        if (width, height) != (480, 360):
                            raise RuntimeError(f"Resize did not complete: {width}x{height}")
                        x, y = int(geometry["X"]), int(geometry["Y"])
                        resized = ImageGrab.grab(xdisplay=env["DISPLAY"]).convert("RGB").crop(
                            (x, y, x + width, y + height))
                        resized.save(directory / "resized.png")
                        # The old canvas covered x=475. It must become the new margin.
                        for point, expected in [((475, 180), (24, 24, 24)), ((240, 180), (255, 107, 24))]:
                            if any(abs(a - b) > 2 for a, b in zip(resized.getpixel(point), expected)):
                                raise RuntimeError(f"Resize left stale drawing: see {directory / 'resized.png'}")
                        result["resize"] = {"size": [width, height], "screenshot": str(directory / "resized.png")}
                    return result
            finally:
                stop(app)
                stop(display)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", default="artifacts/lean-native")
    parser.add_argument("--variant", action="append", choices=FEATURES)
    parser.add_argument("--seconds", type=float, default=2.0)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--build-only", action="store_true")
    mode.add_argument("--measure-only", action="store_true", help="Sample recorded binaries without invoking Cargo")
    args = parser.parse_args()
    if args.seconds <= 0:
        parser.error("--seconds must be positive")
    if not args.build_only and os.environ.get("_FIRE_UI_LEAN_PRIVATE_BUS") != "1":
        env = {**os.environ, "_FIRE_UI_LEAN_PRIVATE_BUS": "1"}
        raise SystemExit(subprocess.run(["dbus-run-session", "--", sys.executable, *sys.argv], env=env).returncode)
    directory = Path(args.output).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    rustc = output(["rustc", "-vV"])
    host = next(line.removeprefix("host: ") for line in rustc.splitlines() if line.startswith("host: "))
    results = {"rustc": rustc, "backend": "X11 / Xvfb / Mesa software GL", "window": [640, 480],
               "workload": "same draw-only app; no fonts, no retained framebuffer, optional services unused",
               "variants": {}}
    if args.measure_only:
        results = json.loads((directory / "results.json").read_text())
    for variant in args.variant or FEATURES:
        variant_dir = directory / variant
        variant_dir.mkdir(parents=True, exist_ok=True)
        if args.measure_only:
            result = results["variants"][variant]
            binary = Path(result["binary"])
            if hashlib.sha256(binary.read_bytes()).hexdigest() != result["binary_sha256"]:
                raise RuntimeError(f"{variant} binary changed; rebuild to refresh its measurements")
        else:
            print(f"Building {variant} in its own consumer workspace...", flush=True)
            binary, result = build(variant, variant_dir, host)
        result["consumer_source_sha256"] = hashlib.sha256(
            (variant_dir / "consumer/src/main.rs").read_bytes()).hexdigest()
        result["lock_sha256"] = hashlib.sha256(
            (variant_dir / "consumer/Cargo.lock").read_bytes()).hexdigest()
        if not args.build_only:
            result["idle"] = sample(binary, variant_dir, args.seconds)
        results["variants"][variant] = result
        (directory / "results.json").write_text(json.dumps(results, indent=2) + "\n")
        print(json.dumps({variant: result}, indent=2), flush=True)


if __name__ == "__main__":
    main()
