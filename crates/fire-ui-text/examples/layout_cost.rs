//! Isolated CPU/allocation experiment, not native memory acceptance.
use fire_ui::{TextEngine, TextRequest, TextStyle};
use fire_ui_fonts::Fonts;
use fire_ui_text::Text;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};
struct Counted;
static CALLS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size as u64, Ordering::Relaxed);
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counted = Counted;
fn main() {
    let rounds: u64 = std::env::args()
        .nth(1)
        .map(|s| s.parse().unwrap())
        .unwrap_or(100);
    let phrase =
        "Fire Notes café e\u{0301} Привет العربية שלום. A useful note with ordinary words.\n";
    let mut value = phrase.repeat(8192 / phrase.len() + 1);
    let mut end = 8192;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value.push_str(&" ".repeat(8192 - end));
    let source: Arc<str> = value.into();
    let fonts = Fonts::load(&["/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"]).unwrap();
    let mut text = Text::new(fonts).unwrap();
    for width in [568., 556.] {
        let request = |revision| TextRequest {
            previous: None,
            text: source.clone(),
            style: TextStyle { size: 16., font: 0 },
            width: Some(width),
            revision,
        };
        drop(text.layout(request(0)));
        CALLS.store(0, Ordering::Relaxed);
        BYTES.store(0, Ordering::Relaxed);
        let started = Instant::now();
        let mut checksum = 0u64;
        for revision in 1..=rounds {
            let p = text.layout(request(revision));
            for line in &p.lines {
                for run in &line.runs {
                    for g in run.glyphs.iter() {
                        checksum = checksum
                            .wrapping_add(g.index as u64)
                            .wrapping_add(g.offset.x.to_bits() as u64);
                    }
                }
            }
            std::hint::black_box(p);
        }
        let elapsed = started.elapsed();
        println!("bytes=8192 width={width} rounds={rounds} mean_us={} allocations_per_layout={} allocated_bytes_per_layout={} checksum={checksum}",elapsed.as_micros() as u64/rounds,CALLS.load(Ordering::Relaxed)/rounds,BYTES.load(Ordering::Relaxed)/rounds);
    }
}
