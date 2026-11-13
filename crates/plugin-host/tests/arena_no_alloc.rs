//! 实时环约束的守卫测试（M3-W2 子项 1，B44）：
//! [`plugin_host::arena::PdoArena::publish`] 是写者（实时环）侧 API，**不得分配内存**。
//!
//! 用计数分配器测量发布前后的分配次数；本文件只有这一个测试，避免并行测试干扰计数。

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use plugin_host::arena::PdoArena;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn publish_path_does_not_allocate() {
    let arena = PdoArena::new(64, 64).expect("构造 arena");
    let sm2 = [0x11u8; 64];
    let sm3 = [0x22u8; 64];

    // 预热：第一次发布也走同一路径，但确保后续测量不受一次性初始化影响。
    arena.publish(&sm2, &sm3).expect("预热发布");

    let before = ALLOCATIONS.load(Ordering::SeqCst);
    for _ in 0..1_000 {
        arena.publish(&sm2, &sm3).expect("发布");
    }
    let after = ALLOCATIONS.load(Ordering::SeqCst);
    assert_eq!(
        after - before,
        0,
        "实时环发布路径分配了 {} 次内存",
        after - before
    );

    // 失败路径（超容量）同样不应分配。
    let too_large = [0u8; 65];
    let before = ALLOCATIONS.load(Ordering::SeqCst);
    for _ in 0..1_000 {
        assert!(arena.publish(&too_large, &sm2).is_err());
    }
    let after = ALLOCATIONS.load(Ordering::SeqCst);
    assert_eq!(
        after - before,
        0,
        "发布失败路径分配了 {} 次内存",
        after - before
    );

    // 收尾：读者侧拷贝允许分配（插件 worker 非实时）。
    assert_eq!(arena.snapshot().generation, 1_001);
}
