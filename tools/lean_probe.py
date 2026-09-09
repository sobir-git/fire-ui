#!/usr/bin/env python3
"""Compare the same draw-only app with minimal and optional native dependencies.

Each variant is an independent consumer workspace and has its own Cargo target
directory: studio features cannot contaminate the minimal measurement. Scratch
workspaces and targets are removed; only explicit evidence binaries and logs remain.
Runtime samples require Linux, Xvfb,
xdotool and dbus-run-session; Pillow enables screenshots. No user display is used.
"""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

from probe_lifetime import install_signal_cleanup, run_owned


ROOT = Path(__file__).resolve().parents[1]
FEATURES = {
    "minimal": ["x11"],
    "extras": ["x11", "accessibility", "inspection", "clipboard", "dialogs"],
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
    with tempfile.TemporaryDirectory(prefix="fire-ui-lean-build-") as temporary:
        return build_in(variant, directory, host, Path(temporary))


def build_in(variant, directory, host, temporary):
    project = temporary / "consumer"
    (project / "src").mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT / "crates/fire-ui-cairo/examples/minimal.rs", project / "src/main.rs")
    # JSON string escaping is also valid for these TOML basic strings.
    (project / "Cargo.toml").write_text(
        '[package]\nname = "fire-ui-lean-probe"\nversion = "0.0.0"\nedition = "2021"\n'
        '[workspace]\n[dependencies]\n'
        f'fire-ui = {{ path = {json.dumps(str(ROOT / "crates/fire-ui"))} }}\n'
        f'fire-ui-native = {{ path = {json.dumps(str(ROOT / "crates/fire-ui-native"))}, '
        f'default-features = false, features = {json.dumps(FEATURES[variant])} }}\n'
        f'fire-ui-fonts = {{ path = {json.dumps(str(ROOT / "crates/fire-ui-fonts"))} }}\n'
        f'fire-ui-cairo = {{ path = {json.dumps(str(ROOT / "crates/fire-ui-cairo"))}, features = ["x11"] }}\n'
        '[profile.release]\nopt-level = 3\nlto = "thin"\ncodegen-units = 1\nstrip = true\n'
    )
    target = temporary / "target"
    had_cache = target.exists()
    env = {**os.environ, "CARGO_TARGET_DIR": str(target)}
    started = time.monotonic()
    with (directory / "build.log").open("w") as log:
        result = run_owned(["cargo", "build", "--release", "--target", host],
                                cwd=project, env=env, stdout=log, stderr=log)
    elapsed = time.monotonic() - started
    if result.returncode:
        raise RuntimeError(f"{variant} build failed: see {directory / 'build.log'}")
    tree = output(["cargo", "tree", "--locked", "--target", host, "--edges", "normal,build",
                   "--prefix", "none", "--format", "{p}"], cwd=project, env=env)
    (directory / "dependencies.txt").write_text(tree + "\n")
    packages = sorted({line.removesuffix(" (*)") for line in tree.splitlines()})
    names = {line.split()[0] for line in packages}
    gpu = sorted(name for name in names if name in {"fire-ui-gl", "femtovg", "glow", "glutin", "glutin-winit", "wgpu", "naga", "ash"})
    if gpu:
        raise RuntimeError(f"GPU dependencies leaked into {variant}: {gpu}")
    if variant == "minimal":
        unwanted = sorted(name for name in names if name.startswith(("accesskit", "wayland"))
                          or name in {"serde", "serde_json", "arboard", "image", "png", "native-dialog", "zbus", "atspi", "fire-ui-text", "rustybuzz", "memmap2", "unicode-bidi", "unicode-script"})
        if unwanted:
            raise RuntimeError(f"Optional dependencies leaked into minimal: {unwanted}")
    binary = target / host / "release/fire-ui-lean-probe"
    # These are explicit evidence outputs, also used by --measure-only.
    evidence_binary = directory / "fire-ui-lean-probe"
    shutil.copy2(binary, evidence_binary)
    evidence_source = directory / "consumer/src"
    evidence_source.mkdir(parents=True, exist_ok=True)
    shutil.copy2(project / "src/main.rs", evidence_source / "main.rs")
    shutil.copy2(project / "Cargo.lock", directory / "consumer/Cargo.lock")
    shutil.copy2(project / "Cargo.toml", directory / "consumer/Cargo.toml")
    binary = evidence_binary
    return binary, {
        "features": FEATURES[variant], "binary": str(binary),
        "binary_bytes": binary.stat().st_size,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "dependency_packages": len(packages) - 1, "dependency_kind": "normal and build, host target",
        "build_seconds": round(elapsed, 3), "target_directory_existed": had_cache,
        "build_cache": "temporary variant-specific target, deleted after build; Cargo downloads shared",
        "target_bytes": sum(path.stat().st_size for path in target.rglob("*") if path.is_file()),
    }


def process_state(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    status = dict(line.split(":", 1) for line in Path(f"/proc/{pid}/status").read_text().splitlines())
    rollup = dict(line.split(":", 1) for line in
                  Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines()[1:])
    memory = {key: int(rollup.get(key, "0 kB").split()[0]) * 1024 for key in
              ("Rss", "Pss", "Private_Dirty", "Private_Clean", "Shared_Dirty", "Shared_Clean", "Swap", "SwapPss", "Anonymous")}
    return {"ticks": int(fields[11]) + int(fields[12]),
            "rss_bytes": memory["Rss"], "pss_bytes": memory["Pss"],
            "private_dirty_bytes": memory["Private_Dirty"],
            "anonymous_bytes": memory["Anonymous"],
            "private_clean_bytes": memory["Private_Clean"],
            "total_private_resident_bytes": memory["Private_Dirty"] + memory["Private_Clean"],
            "shared_resident_bytes": memory["Shared_Dirty"] + memory["Shared_Clean"],
            "shared_dirty_bytes": memory["Shared_Dirty"], "shared_clean_bytes": memory["Shared_Clean"],
            "swap_bytes": memory["Swap"], "swap_pss_bytes": memory["SwapPss"],
            "threads": int(status["Threads"])}


def mapping_memory(pid):
    mappings = {}
    name = None
    fields = {"Private_Dirty", "Private_Clean", "Anonymous", "Rss", "Pss", "Shared_Clean", "Shared_Dirty", "Swap"}
    for line in Path(f"/proc/{pid}/smaps").read_text().splitlines():
        if re.match(r"^[0-9a-f]+-[0-9a-f]+ ", line):
            parts = line.split(maxsplit=5)
            name = parts[5] if len(parts) == 6 else "[anonymous]"
            mappings.setdefault(name, {field: 0 for field in fields})
        elif name is not None and ":" in line:
            field, value = line.split(":", 1)
            if field in fields:
                mappings[name][field] += int(value.split()[0]) * 1024
    return [{"mapping": name, **values} for name, values in sorted(
        mappings.items(), key=lambda item: item[1]["Private_Dirty"], reverse=True)]


def sample(binary, directory, seconds):
    # Recently linked executable pages can remain file-backed Private_Dirty until
    # writeback. Match the app probe: complete executable I/O before launching.
    with binary.open("rb") as executable:
        os.fsync(executable.fileno())
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
                # A loaded CI runner can take several seconds to hand back the
                # display number; five was not enough.
                for _ in range(600):
                    display_file.seek(0)
                    number = display_file.read().strip()
                    if number:
                        break
                    time.sleep(0.05)
                else:
                    raise RuntimeError("Xvfb did not start within 30s")
                env = {**os.environ, "DISPLAY": ":" + number, "HOME": str(temporary),
                       "XDG_RUNTIME_DIR": str(runtime), "XDG_CONFIG_HOME": str(temporary / "config"),
                       "XDG_DATA_HOME": str(temporary / "data"), "XDG_CACHE_HOME": str(temporary / "cache"),
                       "WINIT_UNIX_BACKEND": "x11", "WINIT_X11_SCALE_FACTOR": "1",
                       "GSETTINGS_BACKEND": "memory"}
                for name in ("WAYLAND_DISPLAY", "FIRE_UI_FONT", "FIRE_UI_INSPECT", "FIRE_UI_PROFILE"):
                    env.pop(name, None)
                subprocess.run(["dbus-update-activation-environment", "DISPLAY", "HOME", "XDG_RUNTIME_DIR",
                                "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "GSETTINGS_BACKEND"],
                               env=env, check=True, timeout=5)
                display_before = process_state(display.pid)
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
                    mappings = {"startup": mapping_memory(app.pid)}
                    before = process_state(app.pid)
                    states = {"startup": before}
                    started = time.monotonic()
                    time.sleep(seconds)
                    elapsed = time.monotonic() - started
                    after = process_state(app.pid)
                    states["idle"] = after
                    result = {**after, "seconds": round(elapsed, 3),
                              "cpu_ticks": after["ticks"] - before["ticks"],
                              "cpu_percent_one_core": round((after["ticks"] - before["ticks"]) /
                                                            os.sysconf("SC_CLK_TCK") / elapsed * 100, 3)}
                    del result["ticks"]
                    try:
                        from PIL import ImageGrab
                    except ImportError as error:
                        raise RuntimeError("Pillow is required for rendering verification") from error
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
                    states["resized"] = process_state(app.pid)
                    mappings["resized"] = mapping_memory(app.pid)
                    result["mapping_bytes"] = mappings
                    result["executable_fsync_before_launch"] = True
                    for state in states.values():
                        state.pop("ticks", None)
                    result["states"] = states
                    result["max_sampled_private_dirty_bytes"] = max(state["private_dirty_bytes"] for state in states.values())
                    result["zero_swap"] = all(state["swap_bytes"] == 0 and state["swap_pss_bytes"] == 0 for state in states.values())
                    display_during = process_state(display.pid)
                    stop(app)
                    app = None
                    time.sleep(0.2)
                    display_after = process_state(display.pid)
                    result["display_server"] = {"before": display_before, "during": display_during, "after": display_after,
                        "note": "Dedicated Xvfb, no compositor. Includes X11 resource cost; no GPU renderer selected."}
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
        raise SystemExit(run_owned(["dbus-run-session", "--", sys.executable, *sys.argv], env=env).returncode)
    directory = Path(args.output).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    rustc = output(["rustc", "-vV"])
    host = next(line.removeprefix("host: ") for line in rustc.splitlines() if line.startswith("host: "))
    results = {"rustc": rustc, "backend": "X11 / Xvfb / Cairo", "window": [640, 480],
               "workload": "same draw-only app; no fonts, no retained framebuffer, optional services unused",
               "scope": "Minimal consumer evidence only; does not verify Fire Notes acceptance",
               "accounting": "Private_Dirty * 1024 matches the Fire Notes probe; all resident/shared/swap values are separate exact byte counts",
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
        if not args.build_only and not result["idle"]["zero_swap"]:
            raise RuntimeError("App swapped during the minimal consumer experiment; evidence was saved")


if __name__ == "__main__":
    install_signal_cleanup()
    main()
