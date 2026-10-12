//! Bounds the resolved runtime target before parser or diagnostic allocation.
#[allow(dead_code)]
#[path = "../src/target.rs"]
mod target;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static LARGEST: Cell<usize> = const { Cell::new(0) };
    static TOTAL: Cell<usize> = const { Cell::new(0) };
}
struct Tracking;
fn record(size: usize) {
    if TRACK.try_with(Cell::get).unwrap_or(false) {
        let _ = LARGEST.try_with(|v| v.set(v.get().max(size)));
        let _ = TOTAL.try_with(|v| v.set(v.get().saturating_add(size)));
    }
}
unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let value = unsafe { System.alloc(layout) };
        record(layout.size());
        value
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let value = unsafe { System.alloc_zeroed(layout) };
        record(layout.size());
        value
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let value = unsafe { System.realloc(ptr, layout, size) };
        record(size);
        value
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOC: Tracking = Tracking;

#[test]
fn runtime_target_byte_boundary_preserves_valid_existing_spellings() {
    let prefix = "arm64-apple-macosx";
    for length in [127, 128] {
        let triple = format!("{prefix}{}", "0".repeat(length - prefix.len()));
        assert_eq!(triple.len(), length);
        assert!(target::require_native_posix64(&triple).is_ok());
    }
    let oversized = format!("{prefix}{}", "0".repeat(129 - prefix.len()));
    assert_eq!(oversized.len(), 129);
    let message = target::require_native_posix64(&oversized).unwrap_err();
    assert_eq!(
        message,
        "unsupported native 64-bit POSIX target: resolved target exceeds 128 bytes"
    );
    let unicode = "λ".repeat(65);
    assert_eq!(unicode.chars().count(), 65);
    assert_eq!(
        target::require_native_posix64(&unicode).unwrap_err(),
        message
    );
    for triple in [
        "arm64-apple-macosx26.0.0",
        "x86_64-apple-darwin23.6.0",
        "aarch64-apple-macos4294967295.0.0",
        "x86_64-pc-linux-gnu",
        "aarch64-unknown-linux-musl",
    ] {
        assert!(target::require_native_posix64(triple).is_ok(), "{triple}");
    }
    assert!(target::Target::parse("arm64-apple-macosx26.0.0").is_err());
}
#[test]
fn oversized_runtime_target_refuses_before_amplified_component_or_error_buffers() {
    let inputs = [
        "x-".repeat(500_000),
        format!("arm64-apple-macosx{}", "1.".repeat(500_000)),
        format!("arm64-apple-macosx{}", "0".repeat(1_000_000)),
    ];
    for input in inputs {
        LARGEST.with(|v| v.set(0));
        TOTAL.with(|v| v.set(0));
        TRACK.with(|v| v.set(true));
        let result = target::require_native_posix64(&input);
        TRACK.with(|v| v.set(false));
        let largest = LARGEST.with(Cell::get);
        let total = TOTAL.with(Cell::get);
        assert_eq!(
            result.unwrap_err(),
            "unsupported native 64-bit POSIX target: resolved target exceeds 128 bytes"
        );
        assert!(largest <= 128, "parser/error allocation was {largest}");
        assert!(total <= 128, "parser/error total allocation was {total}");
    }
}
