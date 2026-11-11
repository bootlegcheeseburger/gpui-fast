//! Fork patch: presentation receipts appended to reusable caller storage.

use crate::{PlatformWindow, Window};

/// Fork patch: one frame's presentation, including the caller's identity if assigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentedFrame {
    /// Fork patch: token assigned before this draw; `None` when no token was assigned.
    pub frame_token: Option<u64>,
    /// Fork patch: the display link's presentation target on the host clock, in nanoseconds.
    pub output_host_time_ns: u64,
    /// Fork patch: actual presentation time on that clock, or zero for a dropped frame.
    pub presented_host_ns: u64,
}

impl Window {
    /// Fork patch: assigns a token to the next draw of this window.
    /// The next draw consumes it, including an untimed or skipped draw. A later
    /// assignment before that draw replaces it. Zero is a valid token.
    pub fn set_frame_token(&mut self, token: u64) {
        self.platform_window.set_frame_token(token);
    }

    /// Fork patch: appends token-bearing receipts to reusable caller storage.
    /// The timestamp-only `take_presented` and `drain_presented` APIs remain available;
    /// all receipt readers consume the same queue, so use one reader per window.
    pub fn drain_presented_frames(&mut self, output: &mut Vec<PresentedFrame>) {
        self.platform_window.drain_presented_frames(output);
    }

    /// Fork patch: appends presentation receipts, oldest first, to `output`.
    /// Clear `output` between drains to reuse both buffers after warm-up.
    /// See [`Window::take_presented`] for the timestamp and dropped-frame semantics.
    pub fn drain_presented(&mut self, output: &mut Vec<(u64, u64)>) {
        self.platform_window.drain_presented(output);
    }
}

// Fork patch: backends without a receipt queue retain the existing empty default.
pub(crate) fn drain_unreported<W: PlatformWindow + ?Sized>(
    window: &W,
    output: &mut Vec<(u64, u64)>,
) {
    output.extend(window.take_presented());
}

// Fork patch: existing backends can continue to report untokened timing pairs.
pub(crate) fn drain_frames_unreported<W: PlatformWindow + ?Sized>(
    window: &W,
    output: &mut Vec<PresentedFrame>,
) {
    output.extend(
        window
            .take_presented()
            .into_iter()
            .map(|(target, presented)| PresentedFrame {
                frame_token: None,
                output_host_time_ns: target,
                presented_host_ns: presented,
            }),
    );
}

// Fork patch: a backend without presentation feedback has no token to retain.
pub(crate) fn ignore_frame_token() {}
