//! Fork patch: presentation receipt forwarding for the window's renderer.

use crate::window::MacWindowState;
use parking_lot::Mutex;
use std::rc::Rc;

pub(crate) fn drain_presented(state: &Rc<Mutex<MacWindowState>>, output: &mut Vec<(u64, u64)>) {
    state.lock().renderer.drain_presented(output);
}
