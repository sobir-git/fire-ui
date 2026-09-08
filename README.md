# Fire UI

A small Rust UI framework for persistent widgets, custom drawing and optional
higher-level composition. Use it for native desktop tools, animated text and
interactive canvases. Widgets retain their state; explicit invalidation and frame
requests let the event loop sleep when idle.

**Version 0.5.0 · Rust 1.88+ · MIT · experimental**

| Crate | Responsibility |
| --- | --- |
| `fire-ui` | Typed widget ownership, layout, input, scheduling, text and painting contracts. No dependencies. |
| `fire-ui-widgets` | Controls, design tokens, layout policies, editor, scrolling, menus and virtual lists. |
| `fire-ui-native` | Desktop windows, OpenGL rendering, font shaping, clipboard, IME and accessibility. |

The layers are separate dependencies. You can use the core with your own host,
write custom widgets, or compose the supplied controls. No Fire Notes checkout,
application state or prescribed visual theme is required.

## Start an app

Use the `v0.5.0` GitHub prerelease directly. The three crate archives are attached
to the release; these crates have not been published to crates.io.

```toml
[dependencies]
fire-ui = { git = "https://github.com/sobir-git/fire-ui", tag = "v0.5.0" }
fire-ui-widgets = { git = "https://github.com/sobir-git/fire-ui", tag = "v0.5.0" }
fire-ui-native = { git = "https://github.com/sobir-git/fire-ui", tag = "v0.5.0" }
```

```rust,no_run
use fire_ui::Element;
use fire_ui_native::{run, WindowOptions};
use fire_ui_widgets::{Label, Padding};

fn main() -> Result<(), String> {
    run(
        Padding::new(Element::leaf(Label::new("Hello, Fire UI")), 24.0),
        WindowOptions {
            font: Some(fire_ui_native::system_font().ok_or("Choose a font file")?),
            ..WindowOptions::default()
        },
    )
}
```

Copy [hello.rs](crates/fire-ui-native/examples/hello.rs) into a new Cargo app,
or run either example here:

```sh
cargo run --release -p fire-ui-native --example hello
cargo run --release -p fire-ui-studio
```

The studio is a navigable gallery of everything the framework ships: controls,
text editing, a searchable 100,000-row virtual list, a custom animated canvas, a
live layout playground and the design tokens themselves, with a light/dark switch.
It uses public APIs only — no literal colour, font size or panel coordinate appears
anywhere in it.

The native host defaults to X11 rendering. Accessibility, agent inspection, clipboard,
dialogs and bitmap-font decoding are independent Cargo features. Font coverage and
retained repainting are explicit app choices. See [native configuration](crates/fire-ui-native/README.md).

## Support and limits

The core and widgets are platform-independent Rust with `std`. The native host
targets Linux, macOS and Windows. Linux X11 interactions are verified using Xvfb;
macOS and Windows have CI build/test checks, not verified desktop interaction
parity. Browser, mobile, embedded and `no_std` hosts are not provided.

Supply a licensed TrueType font with `WindowOptions::font` for predictable app
packaging. The host loads no fonts by default. Call `system_font()` explicitly
for a common system face or the `FIRE_UI_FONT` override. `fallback_fonts` supplies ordered fallback faces. Mixed-direction
text shares shaping, selection and visual caret geometry. IME composition carries its
selection and replaces the selected text in a temporary display layout.

[Agent inspection](docs/inspection.md) uses the same semantic actions as accessibility.
It exposes widget identity, values, bounds, supported actions, selection and composition
through the optional `inspection` feature and Unix socket. The `accessibility` feature
enables AccessKit, which publishes text runs and selection to native
assistive technology. Linux implements all six AT-SPI EditableText methods with atomic
range edits, undo and asynchronous clipboard reads. Native probes use installed IBus
Cangjie5/XIM and Orca, including its generated speech. Windows/macOS native IME and
screen-reader verification, and variable-height lists, remain future work.

The [performance report](docs/performance.md) records native measurements and their
limits. Low idle CPU, memory use, direct pointer response and fast resizing remain
requirements. Apps can opt into `WindowOptions::partial_repaint` to retain unchanged pixels
and repaint damaged regions; layout changes repaint the window. Hardware-GPU measurements are still needed.

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
