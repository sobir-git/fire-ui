# Performance baseline

The first rebuild stops scheduling frames when idle. Native rendering still needs work before the framework meets its low-memory and active-CPU goals.

These measurements used the release studio on Linux with Xvfb and Mesa software OpenGL. They describe this environment. Hardware GPU performance has not been measured. CPU percentages count one fully occupied core as 100%, so a multithreaded renderer can exceed 100%.

## Native window

The default renderer paints the window when dirty and does not retain an extra backing framebuffer. The release binary was 4,588,360 bytes. The process had 34 threads during these samples.

| Activity | Sample duration | CPU, one core | Process RSS |
| --- | ---: | ---: | ---: |
| Idle, no focused editor | 3 s | 0%, zero CPU ticks | 143,852 KiB |
| Focused blinking caret | 3 s | 4.33% | 129,076 KiB |
| Fire animation | 3 s | 121.66% | 132,992 KiB |
| Paused again | 3 s | 0%, zero CPU ticks | 133,104 KiB |
| Paddle game | 1 s | 140.99% | 133,220 KiB |

These short samples establish a baseline, not a long-duration idle guarantee. RSS includes the application, font caches, native libraries, and software graphics driver. The measurements do not separate their contributions.

The probe exercised text insertion at the caret, modal opening and dismissal, animation, game input, and repeated resizing. Screenshots were inspected after those actions.

## Resizing

For 59 rendered resize samples, time from the latest resize event received by the application to completion of buffer swap was:

| Metric | Time |
| --- | ---: |
| Median | 12.515 ms |
| 95th percentile | 28.349 ms |
| Maximum | 33.435 ms |

This includes application layout, drawing, and swap. It excludes time before the application receives the event and does not measure when the compositor displays the frame. The 95th percentile exceeds a 60 Hz frame budget, so resize performance still needs attention.

The separate CPU layout probe used a 22,500-byte document and 120 sizes with the native font service. Its median was 0.773 ms, 95th percentile 0.833 ms, and maximum 1.256 ms. Peak process RSS was 4,308 KiB. This process creates no window or GL renderer; its memory use cannot be substituted for the native application's RSS.

Resize events update pending dimensions. The native host resizes GPU storage at the next rendered frame. A regression test verifies that 100 core resize requests before layout result in one layout using the final size.

## Optional retained framebuffer

`WindowOptions::retain_surface` enables damage-based repaint using an extra backing framebuffer. A development probe measured about 155–156 MiB RSS with that option, compared with about 126–140 MiB in the default run above. It did not consistently lower active CPU in software GL. It remains opt-in. Compare both paths on the target hardware before choosing it for an application.

## Reproducing the measurements

```sh
cargo build --release -p fire-ui-studio --examples --bin fire-ui-studio
python3 tools/native_probe.py
/usr/bin/time -f 'peak_rss_kib=%M' target/release/examples/resize_probe
```

The native probe requires Xvfb, xdotool, and Pillow with XCB support. It starts an isolated display and cleans up its own processes. Raw measurements, render timings, and screenshots are written to the ignored `artifacts/` directory. Use `--seconds 30` for longer activity samples and `--retained` to compare the optional framebuffer.

## Remaining work

- Profile native rendering on a hardware GPU, including allocations and resize stalls.
- Reduce work for caret blinking and animation without making window-sized caches mandatory.
- Measure long sessions and large documents; current text layout rebuilds after edits.
- Test actual compositor presentation and interactive resize on Wayland and X11.
- Repeat the same scenarios on low-end target hardware before setting resource budgets.

## Optimization pass

The next pass reduced plain-background drawing cost, removed the request for an unused depth buffer, reused text measurements across resizes that do not change wrapping, and combined queued mouse moves before rendering. The driver may still choose a depth/stencil format internally. No memory reduction is claimed from changing that request.

The final comparison ran the saved original binary and the optimized release sequentially on the same isolated-display setup, with no build running during either probe. Most CPU samples lasted five seconds; the game CPU sample lasted one second. Each paddle test alternated between 20 positions and verified the resulting paddle pixels.

| Measurement | Before | After |
| --- | ---: | ---: |
| Idle CPU ticks, 5 s | 0 | 0 |
| Focused caret CPU, one core | 3.6% | 3.0% |
| Fire animation CPU, one core | 125.0% | 104.6% |
| Game CPU, one core | 126.0% | 98.0% |
| Paddle move to visible pixels, median | 17.120 ms | 15.635 ms |
| Paddle move to visible pixels, 95th percentile | 23.194 ms | 19.451 ms |
| Resize event to completed swap, median | 6.346 ms | 6.280 ms |
| Resize event to completed swap, 95th percentile | 8.854 ms | 8.505 ms |
| Game process RSS | 131,300 KiB | 131,632 KiB |
| Release binary | 4,588,360 bytes | 4,596,000 bytes |

The CPU reduction repeated across runs. Pointer measurements varied more: an earlier pair had medians of 16.324 and 12.520 ms, but its 95th percentile increased from 21.133 to 22.964 ms. The current changes improve typical response; these short samples do not establish that latency spikes are resolved. The paddle measurement includes process launch overhead for each xdotool command and screenshot capture overhead. It is not physical mouse-to-monitor latency.

The resize probe now distinguishes two cases. Its original 600–895 pixel widths keep all document lines unwrapped, so the optimized editor reuses measurements and the measured layout time is below 0.001 ms. A new 240–535 pixel scenario forces rewrapping: two runs measured medians around 1.15–1.18 ms and 95th percentiles around 1.48–2.14 ms. Peak RSS in the latter probe was 4,196 KiB. Skipping unchanged layout is different from making reshaping free.

Initial, editing, and modal screenshots matched the original pixel for pixel with the default renderer. The retained framebuffer mode differed by at most one color level in those screenshots, from its additional compositing pass. Its active CPU and memory were higher than the default renderer in this environment, so it remains optional.

Validation passed with 20 tests, workspace Clippy with warnings denied, formatting, and the native interaction probe. The new resize tests cover cached measurements, crossing the wrapping threshold, and keeping the caret visible when only window height changes. Native text tests also cover mixed RTL/LTR text and grapheme boundaries.

Raw comparison data is preserved in [before.json](benchmarks/optimization-before.json) and [after.json](benchmarks/optimization-after.json). The probe accepts `--output` to keep independent runs. `FIRE_UI_PROFILE=1` also records frame-building, buffer-swap, and pointer-event-to-swap timings in the native process log.
