# Fire UI

A small Rust UI framework for persistent widgets, custom drawing and optional
higher-level composition. Use it for native desktop tools, animated text and
interactive canvases. Widgets retain their state; explicit invalidation and frame
requests let the event loop sleep when idle.

**Version 0.2.0 · Rust 1.88+ · MIT · experimental**

| Crate | Responsibility |
| --- | --- |
| `fire-ui` | Typed widget ownership, layout, input, scheduling, text and painting contracts. No dependencies. |
| `fire-ui-widgets` | Controls, themes, editor, scrolling, menus and virtual lists. |
| `fire-ui-native` | Desktop windows, OpenGL rendering, font shaping, clipboard, IME and accessibility. |

The layers are separate dependencies. You can use the core with your own host,
write custom widgets, or compose the supplied controls. No Fire Notes checkout,
application state or prescribed visual theme is required.

## Start an app

The GitHub release can be used directly. These crates are prepared for crates.io,
but have not been published there yet.

```toml
[dependencies]
fire-ui = { git = "https://github.com/sobir-git/fire-ui", tag = "v0.2.0" }
fire-ui-widgets = { git = "https://github.com/sobir-git/fire-ui", tag = "v0.2.0" }
fire-ui-native = { git = "https://github.com/sobir-git/fire-ui", tag = "v0.2.0" }
```

```rust,no_run
use fire_ui::Element;
use fire_ui_native::{run, WindowOptions};
use fire_ui_widgets::{Label, Padding};

fn main() -> Result<(), String> {
    run(
        Padding::new(Element::leaf(Label::new("Hello, Fire UI")), 24.0),
        WindowOptions::default(),
    )
}
```

Copy [hello.rs](crates/fire-ui-native/examples/hello.rs) into a new Cargo app,
or run either example here:

```sh
cargo run --release -p fire-ui-native --example hello
cargo run --release -p fire-ui-studio
```

The studio includes independent counters, an editor, a searchable 100,000-item
virtual list, animated text and a pointer-controlled paddle. It uses public APIs.

## Support and limits

The core and widgets are platform-independent Rust with `std`. The native host
targets Linux, macOS and Windows. Linux X11 interactions are verified using Xvfb;
macOS and Windows have CI build/test checks, not verified desktop interaction
parity. Browser, mobile, embedded and `no_std` hosts are not provided.

Supply a licensed TrueType font with `WindowOptions::font` for predictable app
packaging. The host otherwise searches common system fonts; `FIRE_UI_FONT`
overrides that search. Full bidi editing, complete editable-text accessibility,
platform IME coverage, variable-height lists and partial repaint remain unfinished.

The [performance report](docs/performance.md) records native measurements and their
limits. Low idle CPU, memory use, direct pointer response and fast resizing remain
requirements. Software-rendered animation is still expensive.

## Development

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo doc --workspace --no-deps --locked
cargo package --workspace --exclude fire-ui-studio --locked
```

`cargo doc --open -p fire-ui --no-deps` opens the public API reference locally.
CI builds/tests on Linux, macOS and Windows and checks the minimum Rust version.
The Linux native probe uses Xvfb, xdotool, Pillow, fonts and a private accessibility bus:

```sh
cargo build --release -p fire-ui-studio
python3 tools/native_probe.py --seconds 2 --accessibility --output artifacts/native
```

[Fire Notes](https://github.com/sobir-git/fire-notes) is an external consumer.
For local co-development, keep its checkout beside Fire UI and use path dependencies.

## Versioning

All crates share `workspace.package.version`. Keep the three internal dependency
versions in sync. This is early ideation: breaking redesigns are expected. Use a
new 0.x minor version for breaking changes and patch versions for compatible fixes.
No compatibility shims are required. Pin a Git tag when you need a fixed snapshot.

Release by running the checks above, committing, tagging `vVERSION`, and attaching
`target/package/*.crate` to the GitHub release. Registry publication is separate:
publish `fire-ui`, then `fire-ui-widgets`, then `fire-ui-native` after each dependency
is available. Never publish the studio or replace an existing release tag.

See [architecture](docs/architecture.md), [changelog](CHANGELOG.md) and [project rules](AGENTS.md).
MIT licensed; see [LICENSE](LICENSE).
