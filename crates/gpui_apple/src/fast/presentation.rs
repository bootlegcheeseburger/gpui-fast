//! Fork patch: the renderer owns its receipt queue; draining preserves its storage.

use crate::metal_renderer::MetalRenderer;

impl MetalRenderer {
    /// Fork patch: appends receipts without giving away the callback queue's allocation.
    pub fn drain_presented(&self, output: &mut Vec<(u64, u64)>) {
        drain_queue(&mut self.presented.lock(), output);
    }
}

fn drain_queue(queue: &mut Vec<(u64, u64)>, output: &mut Vec<(u64, u64)>) {
    output.extend(queue.drain(..).inspect(|_| {
        #[cfg(test)]
        tests::MOVED.with(|n| n.set(n.get() + 1));
    }));
}

#[cfg(test)]
mod tests {
    use super::drain_queue;
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };

    thread_local! {
        static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
        pub(super) static MOVED: Cell<usize> = const { Cell::new(0) };
    }
    struct Allocator;
    #[global_allocator]
    static ALLOCATOR: Allocator = Allocator;

    // Fork patch: SAFETY: all allocation operations delegate unchanged to System.
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOCATIONS.with(|n| n.set(n.get().map(|n| n + 1)));
            // Fork patch: SAFETY: the caller supplies a valid allocation layout.
            unsafe { System.alloc(layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            ALLOCATIONS.with(|n| n.set(n.get().map(|n| n + 1)));
            // Fork patch: SAFETY: the caller supplies a valid allocation layout.
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            ALLOCATIONS.with(|n| n.set(n.get().map(|n| n + 1)));
            // Fork patch: SAFETY: the caller supplies a valid allocation and new size.
            unsafe { System.realloc(ptr, layout, size) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // Fork patch: SAFETY: the allocation and layout came from this allocator.
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    fn cost(size: usize) -> usize {
        let mut queue = Vec::with_capacity(size);
        let mut output = Vec::with_capacity(size + 1);
        let queue_ptr = queue.as_ptr();
        let output_ptr = output.as_ptr();
        MOVED.with(|n| n.set(0));
        ALLOCATIONS.with(|n| n.set(Some(0)));
        for _ in 0..8 {
            queue.extend((0..size as u64).map(|i| (i, i + 1)));
            output.push((99, 100));
            drain_queue(&mut queue, &mut output);
            assert!(queue.is_empty());
            assert_eq!(queue.capacity(), size, "receipt queue lost its allocation");
            assert_eq!(queue.as_ptr(), queue_ptr);
            assert_eq!(output.as_ptr(), output_ptr);
            assert_eq!(output.len(), size + 1);
            assert_eq!(output[0], (99, 100));
            assert_eq!(output[1], (0, 1));
            assert_eq!(output[size], (size as u64 - 1, size as u64));
            output.clear();
        }
        drain_queue(&mut queue, &mut output);
        let allocations = ALLOCATIONS.with(|n| n.replace(None).unwrap());
        assert_eq!(
            allocations, 0,
            "warm queue and caller buffer must not allocate"
        );
        MOVED.with(Cell::get)
    }

    #[test]
    fn drain_keeps_capacity_and_scales_linearly_without_allocating() {
        let small = cost(64);
        let large = cost(256);
        assert_eq!(small, 8 * 64);
        assert_eq!(large, 4 * small);
    }
}
