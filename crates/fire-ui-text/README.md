# Fire UI text

Unicode shaping independently of native windows and rendering. Font storage comes
from `fire-ui-fonts`; `Text::new(fonts)` validates the selected faces and returns a
`Result`. Share the same ordered font resources with the renderer.

Shaping plans, parsed faces and a reusable buffer live only for one layout call.
They are shared across that paragraph's runs; no document cache survives the call.

`cargo run --release -p fire-ui-text --example layout_cost -- 200` measures the
frozen 8 KiB mixed-script note at 568 and 556 logical pixels, using DejaVu Sans Mono
at 16 pixels. On the development machine, plan/buffer reuse and avoiding redundant
single-face coverage queries changed these 200-layout averages:

| Width | Before µs | After µs | Before allocated bytes | After allocated bytes |
| --- | ---: | ---: | ---: | ---: |
| 568 | 6,357 | 3,186 | 18,486,512 | 2,773,028 |
| 556 | 6,437 | 3,217 | 18,574,904 | 2,861,420 |

Glyph/position checksums were identical. Allocation totals count requests over a
layout, not retained or resident memory. This experiment does not verify native
editor latency or the app memory requirement. Hardware profiling was unavailable
under this machine's perf permissions.

`TextRequest::previous` borrows a caller-owned layout to reuse unchanged logical
lines after edits. Changed metrics and wrapping are checked before reuse. The
engine retains no document cache; dropping the caller's paragraph releases it.
