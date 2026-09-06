//! CPU-only native-font layout measurements; no window or GPU required.
use fire_ui::*;
use fire_ui_native::NativeText;
use fire_ui_widgets::{TextInput, Theme};
use std::time::Instant;

fn main() {
    let mut text = NativeText::new().expect("font");
    let content =
        "Resize should preserve wrapping, selection, and the location of the caret.\n".repeat(300);
    let bytes = content.len();
    let mut ui = Ui::new(
        Node::new(TextInput::editor(content, Theme::default())),
        Size::new(800., 600.),
    );
    ui.layout(&mut text);
    println!("{{\"text_bytes\":{bytes},\"scenarios\":[");
    probe(&mut ui, &mut text, "wide_cached", 600.);
    println!(",");
    probe(&mut ui, &mut text, "narrow_reflow", 240.);
    println!("]}}");
}

fn probe(ui: &mut Ui, text: &mut NativeText, name: &str, min_width: f32) {
    let mut samples = Vec::new();
    for step in 0..120 {
        let started = Instant::now();
        ui.resize(Size::new(
            min_width + (step % 60) as f32 * 5.,
            500. + (step % 30) as f32,
        ));
        ui.layout(text);
        samples.push(started.elapsed().as_secs_f64() * 1000.);
    }
    samples.sort_by(f64::total_cmp);
    print!("{{\"name\":\"{name}\",\"resize_samples\":120,\"layout_p50_ms\":{:.6},\"layout_p95_ms\":{:.6},\"layout_max_ms\":{:.6}}}", samples[60], samples[114], samples[119]);
}
