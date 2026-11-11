//! Fork patch: window identity, propagation and dispatch budgets on the shared global route.

use crate::{
    AppContext as _, Context, Empty, FocusHandle, InteractiveElement as _, IntoElement, Render,
    TestAppContext, Window, WindowHandle, div,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Debug, Default, PartialEq, crate::Action)]
struct Probe;

struct Root {
    focus: FocusHandle,
    calls: Rc<RefCell<Vec<u8>>>,
    consume: Rc<Cell<bool>>,
}
impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let calls = self.calls.clone();
        let consume = self.consume.clone();
        div()
            .track_focus(&self.focus)
            .on_action(move |_: &Probe, _, cx| {
                calls.borrow_mut().push(0);
                if !consume.get() {
                    cx.propagate();
                }
            })
    }
}

#[test]
fn one_window_sees_itself_after_local_bubbling() {
    let mut cx = TestAppContext::single();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let consume = Rc::new(Cell::new(false));
    let window = cx.add_window(|_, cx| Root {
        focus: cx.focus_handle(),
        calls: calls.clone(),
        consume: consume.clone(),
    });
    window
        .update(&mut cx, |root, window, cx| window.focus(&root.focus, cx))
        .unwrap();
    let seen = Rc::new(Cell::new(None));
    cx.update(|cx| {
        let legacy_calls = calls.clone();
        cx.on_action::<Probe>(move |_, cx| {
            legacy_calls.borrow_mut().push(1);
            cx.propagate();
        });
        let calls = calls.clone();
        let seen = seen.clone();
        cx.on_action_in_window::<Probe>(move |_, window, cx| {
            calls.borrow_mut().push(2);
            seen.set(Some(
                window
                    .expect("window dispatch must retain its window")
                    .window_handle()
                    .window_id(),
            ));
            cx.propagate();
        });
        assert!(cx.is_action_available(&Probe));
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
        window.dispatch_action(Box::new(Probe), cx);
    })
    .unwrap();
    assert_eq!(seen.get(), Some(window.window_id()));
    assert_eq!(&*calls.borrow(), &[0, 2, 1]);
    calls.borrow_mut().clear();
    consume.set(true);
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_action(Box::new(Probe), cx)
    })
    .unwrap();
    assert_eq!(&*calls.borrow(), &[0]);
}

fn empty_window(cx: &mut TestAppContext) -> WindowHandle<Empty> {
    let window = cx.add_window(|_, _| Empty);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    window
}

#[test]
fn two_windows_each_see_their_own_window() {
    let mut cx = TestAppContext::single();
    let first = empty_window(&mut cx);
    let second = empty_window(&mut cx);
    let seen = Rc::new(RefCell::new(Vec::new()));
    cx.update(|cx| {
        let seen = seen.clone();
        cx.on_action_in_window::<Probe>(move |_, window, _| {
            seen.borrow_mut()
                .push(window.expect("window dispatch").window_handle().window_id());
        });
    });
    for window in [first, second, first] {
        cx.update_window(window.into(), |_, window, cx| {
            window.dispatch_action(Box::new(Probe), cx)
        })
        .unwrap();
    }
    assert_eq!(
        &*seen.borrow(),
        &[first.window_id(), second.window_id(), first.window_id()]
    );
}

#[test]
fn windowless_dispatch_sees_none() {
    let cx = TestAppContext::single();
    let calls = Rc::new(Cell::new(0));
    cx.update(|cx| {
        let calls = calls.clone();
        cx.on_action_in_window::<Probe>(move |_, window, _| {
            assert!(window.is_none());
            calls.set(calls.get() + 1);
        });
        cx.dispatch_action(&Probe);
    });
    assert_eq!(calls.get(), 1);
}

fn dispatch_cost(listeners: usize) -> (usize, usize) {
    let mut cx = TestAppContext::single();
    let handle = empty_window(&mut cx);
    let calls = Rc::new(Cell::new(0));
    cx.update(|cx| {
        for _ in 0..listeners {
            let calls = calls.clone();
            cx.on_action_in_window::<Probe>(move |_, window, cx| {
                assert_eq!(
                    window.expect("dispatch window").window_handle().window_id(),
                    handle.window_id()
                );
                calls.set(calls.get() + 1);
                cx.propagate();
            });
        }
    });
    cx.update_window(handle.into(), |_, window, cx| {
        let root = window.rendered_frame.dispatch_tree.root_node_id();
        window.dispatch_action_on_node_inner(root, &Probe, cx);
        calls.set(0);
        crate::fast::global_actions::DISPATCH_WORK.with(|work| work.set(0));
        let allocations = super::view_stats::allocations(|| {
            for _ in 0..64 {
                window.dispatch_action_on_node_inner(root, &Probe, cx);
            }
        });
        assert_eq!(
            allocations, 0,
            "window-aware global dispatch must add no allocations"
        );
    })
    .unwrap();
    (
        calls.get(),
        crate::fast::global_actions::DISPATCH_WORK.with(Cell::get),
    )
}

#[test]
fn dispatch_scales_linearly_without_allocations() {
    let small = dispatch_cost(64);
    let large = dispatch_cost(256);
    assert_eq!(small, (64 * 64, 2 * 64 * 64));
    assert_eq!(large, (4 * small.0, 4 * small.1));
}
