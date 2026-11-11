//! Fork patch: presentation receipt forwarding for the window's renderer.

use crate::window::MacWindowState;
use parking_lot::Mutex;
use std::rc::Rc;

pub(crate) fn drain_presented(state: &Rc<Mutex<MacWindowState>>, output: &mut Vec<(u64, u64)>) {
    state.lock().renderer.drain_presented(output);
}

pub(crate) fn set_frame_token(state: &Rc<Mutex<MacWindowState>>, token: u64) {
    state.lock().renderer.set_frame_token(token);
}

pub(crate) fn drain_presented_frames(
    state: &Rc<Mutex<MacWindowState>>,
    output: &mut Vec<gpui::PresentedFrame>,
) {
    state.lock().renderer.drain_presented_frames(output);
}
