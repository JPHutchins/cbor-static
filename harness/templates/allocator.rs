extern crate alloc;

struct Heap;

unsafe impl core::alloc::GlobalAlloc for Heap {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        black_box((black_box(0x2000_2000usize) as *mut u8, layout)).0
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        black_box((ptr, layout));
    }
}

#[global_allocator]
static HEAP: Heap = Heap;
