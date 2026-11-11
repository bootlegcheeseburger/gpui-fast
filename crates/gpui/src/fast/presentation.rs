//! Fork patch: presentation receipts appended to reusable caller storage.

use crate::{PlatformWindow, Window};

impl Window {
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
