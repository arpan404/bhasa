use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct RequestedAllocationCounter;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn charge(size: usize) {
    let live = LIVE.fetch_add(size, Ordering::SeqCst) + size;
    PEAK.fetch_max(live, Ordering::SeqCst);
}
unsafe impl GlobalAlloc for RequestedAllocationCounter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            charge(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            charge(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::SeqCst);
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() {
            if size >= layout.size() {
                charge(size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - size, Ordering::SeqCst);
            }
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: RequestedAllocationCounter = RequestedAllocationCounter;

#[test]
fn repeated_alias_uses_do_not_retain_one_large_edit_string_each() {
    let mut source = String::from("struct Pair[A,B]{left:A;right:B}\ntype Level0=int\n");
    for index in 1..12 {
        source.push_str(&format!(
            "type Level{index}=Pair[Level{},Level{}]\n",
            index - 1,
            index - 1
        ));
    }
    source.push_str("def main(){\n");
    for index in 0..128 {
        source.push_str(&format!("var value{index}:Level11\n"));
    }
    source.push_str("}\n");
    let input_bytes = source.len();
    let files = vec![bhasa::project::SourceFile {
        path: "main.bh".into(),
        source,
        base: 0,
        imports: vec![],
        import_spans: vec![],
    }];
    let baseline = LIVE.load(Ordering::SeqCst);
    PEAK.store(baseline, Ordering::SeqCst);
    let expansion = bhasa::modules::expand_files(&files).unwrap();
    let peak_extra = PEAK.load(Ordering::SeqCst).saturating_sub(baseline);
    let output_capacity = expansion.source.capacity();
    let output_bytes = expansion.source.len();
    // Charge the necessary rendered output separately. The remaining budget
    // permits metadata/maps/cache, but fails if 128 roughly 36 KiB target
    // replacement strings remain live alongside the rendered output.
    assert!(peak_extra <= output_capacity + 2*1024*1024,
        "input={input_bytes}, output={output_bytes}, output_capacity={output_capacity}, peak_extra={peak_extra}");
    println!("input={input_bytes}, output={output_bytes}, output_capacity={output_capacity}, peak_extra={peak_extra}");
}
