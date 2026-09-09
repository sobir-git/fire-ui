use fire_ui_fonts::{Font, FontBytes, Fonts};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
struct Counting;
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(l.size(), Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOC: Counting = Counting;
fn counts() -> (usize, usize) {
    (CALLS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed))
}
fn difference(before: (usize, usize)) -> (usize, usize) {
    let after = counts();
    (after.0 - before.0, after.1 - before.1)
}
fn main() {
    let before = counts();
    let bytes = FontBytes::from_static(&[1, 2, 3]);
    let static_cost = difference(before);
    let before = counts();
    let fonts = Fonts::new([Font {
        bytes,
        face_index: 0,
    }]);
    let collection_cost = difference(before);
    let before = counts();
    let copy = fonts.clone();
    let clone_cost = difference(before);
    println!("FontBytes size={} Font descriptor size={} static {:?} one-face collection {:?} collection clone {:?}",std::mem::size_of::<FontBytes>(),std::mem::size_of::<Font>(),static_cost,collection_cost,clone_cost);
    std::hint::black_box(copy);
}
