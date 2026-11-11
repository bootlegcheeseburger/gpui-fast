//! Fork patch: global listeners share the existing route and retain its dispatching window.

use crate::{Action, App, DispatchPhase, Window};
use std::{
    any::{Any, TypeId},
    rc::Rc,
};

// Fork patch: count listener visits, including capture, in deterministic scale tests.
#[cfg(test)]
thread_local! {
    pub(crate) static DISPATCH_WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) type Listener = dyn Fn(&dyn Any, DispatchPhase, Option<&mut Window>, &mut App);

impl App {
    /// Fork patch: registers a global action handler at the end of window bubbling.
    /// The callback receives the dispatching window, or `None` for a windowless
    /// dispatch. Like `on_action`, window bubbling stops unless it calls `propagate`.
    pub fn on_action_in_window<A: Action>(
        &mut self,
        listener: impl Fn(&A, Option<&mut Window>, &mut App) + 'static,
    ) -> &mut Self {
        self.global_action_listeners
            .entry(TypeId::of::<A>())
            .or_default()
            .push(Rc::new(move |action, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    listener(action.downcast_ref().unwrap(), window, cx);
                }
            }));
        self
    }
}

pub(crate) fn on_action<A: Action>(
    cx: &mut App,
    listener: impl Fn(&A, &mut App) + 'static,
) -> &mut App {
    cx.on_action_in_window(move |action, _, cx| listener(action, cx))
}

pub(crate) fn call(
    listener: &Rc<Listener>,
    action: &dyn Action,
    phase: DispatchPhase,
    window: Option<&mut Window>,
    cx: &mut App,
) {
    #[cfg(test)]
    DISPATCH_WORK.with(|work| work.set(work.get() + 1));
    listener(action.as_any(), phase, window, cx);
}
