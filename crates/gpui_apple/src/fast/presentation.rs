//! Fork patch: the renderer owns its receipt queue; draining preserves its storage.

use crate::metal_renderer::MetalRenderer;
use block2::RcBlock;
use gpui::PresentedFrame;
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_metal::MTLDrawable;
use objc2_quartz_core::CAMetalDrawable;
use parking_lot::Mutex;
use std::{ptr::NonNull, sync::Arc};

pub(crate) const PRESENTED_KEEP: usize = 256;

pub(crate) type ReceiptQueue = Arc<Mutex<Vec<PresentedFrame>>>;

// Fork patch: the next draw is the sole invalidation point for this caller token.
#[derive(Default)]
pub(crate) struct FrameToken(Option<u64>);

pub(crate) struct PendingFrame {
    token: Option<u64>,
    target: u64,
}

impl FrameToken {
    fn begin_frame(&mut self, target: Option<u64>) -> Option<PendingFrame> {
        let token = self.0.take();
        target.map(|target| PendingFrame { token, target })
    }
}

pub(crate) fn begin_draw(renderer: &mut MetalRenderer) -> Option<PendingFrame> {
    renderer
        .fast_frame_token
        .begin_frame(renderer.frame_output_host_time_ns.take())
}

pub(crate) fn presented_host_ns(seconds: f64) -> u64 {
    if seconds.is_finite() && seconds > 0.0 {
        (seconds * 1e9).round() as u64
    } else {
        0
    }
}

fn record_presented(frame: PendingFrame, queue: &ReceiptQueue, seconds: f64) {
    push_receipt(
        queue,
        PresentedFrame {
            frame_token: frame.token,
            output_host_time_ns: frame.target,
            presented_host_ns: presented_host_ns(seconds),
        },
    );
}

pub(crate) fn track_drawable(
    frame: Option<PendingFrame>,
    queue: &ReceiptQueue,
    drawable: &Retained<ProtocolObject<dyn CAMetalDrawable>>,
) {
    if let Some(frame) = frame {
        let queue = queue.clone();
        let block = RcBlock::new(move |drawable: NonNull<ProtocolObject<dyn MTLDrawable>>| {
            // Fork patch: SAFETY: Metal keeps the drawable live during its presented callback.
            let seconds = unsafe { drawable.as_ref() }.presentedTime();
            record_presented(
                PendingFrame {
                    token: frame.token,
                    target: frame.target,
                },
                &queue,
                seconds,
            );
        });
        // Fork patch: SAFETY: the live block has Metal's declared presented-handler signature.
        unsafe { drawable.addPresentedHandler(RcBlock::as_ptr(&block)) };
    }
}

fn push_receipt(queue: &ReceiptQueue, receipt: PresentedFrame) {
    let mut queue = queue.lock();
    if queue.len() >= PRESENTED_KEEP {
        let excess = queue.len() + 1 - PRESENTED_KEEP;
        queue.drain(..excess);
    }
    queue.push(receipt);
}

#[cfg(test)]
pub(crate) fn queue_legacy(queue: &ReceiptQueue, sample: (u64, u64)) {
    push_receipt(
        queue,
        PresentedFrame {
            frame_token: None,
            output_host_time_ns: sample.0,
            presented_host_ns: sample.1,
        },
    );
}

fn timestamps(frame: PresentedFrame) -> (u64, u64) {
    (frame.output_host_time_ns, frame.presented_host_ns)
}

pub(crate) fn take_legacy(renderer: &MetalRenderer) -> Vec<(u64, u64)> {
    std::mem::take(&mut *renderer.presented.lock())
        .into_iter()
        .map(timestamps)
        .collect()
}

impl MetalRenderer {
    /// Fork patch: the token is consumed by the next draw, including one with no drawable.
    pub fn set_frame_token(&mut self, token: u64) {
        self.fast_frame_token.0 = Some(token);
    }

    /// Fork patch: appends complete receipts while retaining both warmed allocations.
    pub fn drain_presented_frames(&self, output: &mut Vec<PresentedFrame>) {
        drain_queue(&mut self.presented.lock(), output, |frame| frame);
    }

    /// Fork patch: appends receipts without giving away the callback queue's allocation.
    pub fn drain_presented(&self, output: &mut Vec<(u64, u64)>) {
        drain_queue(&mut self.presented.lock(), output, timestamps);
    }
}

fn drain_queue<T, R>(queue: &mut Vec<T>, output: &mut Vec<R>, map: impl FnMut(T) -> R) {
    output.extend(queue.drain(..).map(map).inspect(|_| {
        #[cfg(test)]
        tests::MOVED.with(|n| n.set(n.get() + 1));
    }));
}

#[cfg(test)]
mod tests {
    use super::{FrameToken, ReceiptQueue, drain_queue, record_presented};
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
            drain_queue(&mut queue, &mut output, |sample| sample);
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
        drain_queue(&mut queue, &mut output, |sample| sample);
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
    #[test]
    fn frame_tokens_follow_draws_with_duplicate_targets() {
        let mut token = FrameToken::default();
        let queue = ReceiptQueue::default();
        token.0 = Some(0);
        let first = token.begin_frame(Some(42)).unwrap();
        token.0 = Some(u64::MAX);
        let second = token.begin_frame(Some(42)).unwrap();
        // Fork patch: callbacks may arrive in a different order than draws.
        record_presented(second, &queue, 2.0);
        record_presented(first, &queue, 0.0);
        let mut output = Vec::with_capacity(256);
        drain_queue(&mut queue.lock(), &mut output, |frame| frame);
        assert_eq!(output[0].frame_token, Some(u64::MAX));
        assert_eq!(output[1].frame_token, Some(0));
        assert_eq!(output[0].output_host_time_ns, output[1].output_host_time_ns);
        assert_eq!(output[0].presented_host_ns, 2_000_000_000);
        assert_eq!(output[1].presented_host_ns, 0);
        token.0 = Some(3);
        assert!(token.begin_frame(None).is_none());
        let untokened = token.begin_frame(Some(42)).unwrap();
        record_presented(untokened, &queue, 1.0);
        output.clear();
        drain_queue(&mut queue.lock(), &mut output, |frame| frame);
        assert_eq!(output[0].frame_token, None);
        let capacity = queue.lock().capacity();
        ALLOCATIONS.with(|n| n.set(Some(0)));
        for i in 0..256 {
            token.0 = Some(i);
            record_presented(token.begin_frame(Some(42)).unwrap(), &queue, 1.0);
            output.clear();
            drain_queue(&mut queue.lock(), &mut output, |frame| frame);
            assert_eq!(output[0].frame_token, Some(i));
            assert_eq!(queue.lock().capacity(), capacity);
        }
        let allocations = ALLOCATIONS.with(|n| n.replace(None).unwrap());
        assert_eq!(
            allocations, 0,
            "tokens and warmed full-receipt drains must not allocate"
        );
    }
}
