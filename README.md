# Fire UI

A small Rust UI framework for persistent widgets, custom drawing and optional
higher-level composition. Use it for native desktop tools, animated text and
interactive canvases. Widgets retain their state; explicit invalidation and frame
requests let the event loop sleep when idle.

**Version 0.7.0 · Rust 1.88+ · MIT · experimental**

| Crate | Responsibility |
| --- | --- |
| `fire-ui` | Typed widget ownership, layout, input, scheduling, text and painting contracts. No dependencies. |
| `fire-ui-widgets` | Controls, design tokens, layout policies, editor, scrolling, menus and virtual lists. |
| `fire-ui-native` | Desktop windows, input/IME routing and optional platform services. |
| `fire-ui-fonts` | Shared immutable font bytes and explicit file-face selection; optional mapping. |
| `fire-ui-text` | Unicode shaping and editing geometry. |
| `fire-ui-cairo` | Cairo painting and an optional direct X11 presentation target. |
| `fire-ui-gl` | Optional OpenGL painting and retained repainting. |

The layers are separate dependencies. You can use the core with your own host,
write custom widgets, or compose the supplied controls. No Fire Notes checkout,
application state or prescribed visual theme is required.

## Start an app

The v0.7.0 GitHub prerelease provides seven crate archives; these crates are not
published to crates.io. Apps select a host, text engine and renderer independently. For a
Linux/X11 app, use path dependencies on `fire-ui`, `fire-ui-widgets`,
`fire-ui-native`, `fire-ui-fonts`, `fire-ui-text` and `fire-ui-cairo` with its `x11` feature.

```rust,no_run
use fire_ui::Element;
use fire_ui_native::{run, WindowOptions};
use fire_ui_widgets::{Label, Padding};

fn main() -> Result<(), String> {
    let fonts = fire_ui_fonts::Fonts::load(&["/path/to/app-font.ttf"])?;
    run(
        Padding::new(Element::leaf(Label::new("Hello, Fire UI")), 24.0),
        WindowOptions::default(),
        fire_ui_text::Text::new(fonts.clone())?,
        fire_ui_cairo::Cairo { fonts },
    )
}
```

The [hello example](crates/fire-ui-cairo/examples/hello.rs) explicitly selects a
system font. The [minimal example](crates/fire-ui-cairo/examples/minimal.rs)
draws a custom widget without loading fonts or optional native services.

```sh
cargo run --release -p fire-ui-cairo --features x11 --example hello
cargo run --release -p fire-ui-cairo --features x11 --example minimal
cargo run --release -p fire-ui-studio
```

The studio is a navigable gallery of everything the framework ships: controls,
text editing, a searchable 100,000-row virtual list, a custom animated canvas, a
live layout playground and the design tokens themselves, with a light/dark switch.
It uses public APIs only — no literal colour, font size or panel coordinate appears
anywhere in it.

The native host defaults to X11 windows. Accessibility, agent inspection, clipboard
and dialogs are independent host features. GPU drawing, font coverage and retained
repainting are explicit app choices. OpenGL bitmap-font decoding is a renderer feature. See [native configuration](crates/fire-ui-native/README.md).

## Support and limits

The core and widgets are platform-independent Rust with `std`. The native host
targets Linux, macOS and Windows. Linux X11 interactions are verified using Xvfb;
macOS and Windows have CI build/test checks, not verified desktop interaction
parity. Browser, mobile, embedded and `no_std` hosts are not provided.

Supply explicitly selected, licensed font files through `fire-ui-fonts::Fonts`, or provide owned/static bytes.
`Fonts::load` owns stable bytes; the optional `mmap` feature adds `unsafe Fonts::map`,
which requires the caller to keep the
mapped files immutable for every resource's lifetime. `system_font()` is an
optional helper, including a `FIRE_UI_FONT` override. There is no font discovery
in the native host. The same font resources serve shaping and drawing.
Mixed-direction text shares shaping, selection and visual caret geometry. IME
composition preserves its selection in a temporary display layout.

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
requirements. Cairo draws directly into the X11 window without a client framebuffer;
OpenGL consumers can choose retained repainting on `fire_ui_gl::OpenGl`.
Display-server surfaces, glyph resources and GPU costs are measured separately from
app memory. The full Fire Notes memory requirement has not yet passed reliably.

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

All crates share `workspace.package.version`. Keep all internal dependency
versions in sync. This is early ideation: breaking redesigns are expected. Use a
new 0.x minor version for breaking changes and patch versions for compatible fixes.
No compatibility shims are required. Pin a Git tag when you need a fixed snapshot.

Release by running the checks above, committing, tagging `vVERSION`, and attaching
`target/package/*.crate` to the GitHub release. Registry publication is separate:
publish crates in dependency order after their internal dependencies are available. Never publish the studio or replace an existing release tag.

See [architecture](docs/architecture.md), [changelog](CHANGELOG.md) and [project rules](AGENTS.md).
MIT licensed; see [LICENSE](LICENSE).
