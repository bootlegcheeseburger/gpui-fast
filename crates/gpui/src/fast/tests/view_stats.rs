//! Fork patch: per-view counters, cache classification and instrumentation budgets.

use crate::{
    AppContext as _, Context, Entity, EntityId, IntoElement, ParentElement as _, Render, Style,
    StyleRefinement, Styled as _, TestAppContext, ViewWork, Window, div,
    fast::view_stats::ViewCounters, px, taffy::TaffyLayoutEngine,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::Arc,
};

// Fork patch: count only allocations in an explicitly armed calling-thread scope.
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
struct TestAllocator;
#[global_allocator]
static ALLOCATOR: TestAllocator = TestAllocator;

// Fork patch: SAFETY: every operation delegates to System with its original arguments.
unsafe impl GlobalAlloc for TestAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|count| count.set(count.get().map(|n| n + 1)));
        // Fork patch: SAFETY: the caller supplies a valid allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|count| count.set(count.get().map(|n| n + 1)));
        // Fork patch: SAFETY: the caller supplies a valid allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Fork patch: SAFETY: pointer and layout came from this allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.with(|count| count.set(count.get().map(|n| n + 1)));
        // Fork patch: SAFETY: the caller provides the allocation and valid new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

struct Panel {
    label: String,
}
impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().h(px(20.)).child(self.label.clone())
    }
}
struct Root {
    panels: Vec<Entity<Panel>>,
}
impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .children(self.panels.iter().map(|panel| {
                panel
                    .clone()
                    .cached(StyleRefinement::default().h(px(20.)).w_full())
            }))
    }
}

#[test]
fn cached_view_counts_kept_apart_from_built() {
    let mut cx = TestAppContext::single();
    let panels: Vec<_> = (0..3)
        .map(|i| {
            cx.new(|_| Panel {
                label: format!("panel {i}"),
            })
        })
        .collect();
    let window = cx.add_window(|_, _| Root {
        panels: panels.clone(),
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
        window.reset_layout_stats();
    })
    .unwrap();
    window.update(&mut cx, |_, _, cx| cx.notify()).unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
        for panel in &panels {
            let work = window.view_stats(panel.entity_id());
            assert_eq!(work.built(), 0, "idle cached view: {work:?}");
            assert_eq!(work.laid(), 0, "idle cached view: {work:?}");
            assert!(work.nodes_kept > 0, "idle view's retained nodes: {work:?}");
            assert_eq!(work.measure_calls, 0);
            assert_eq!(work.lines_shaped, 0);
        }
    })
    .unwrap();
}

#[test]
fn planted_window_refresh_reports_built_in_every_view() {
    let mut cx = TestAppContext::single();
    let panels: Vec<_> = (0..3)
        .map(|i| {
            cx.new(|_| Panel {
                label: format!("panel {i}"),
            })
        })
        .collect();
    let window = cx.add_window(|_, _| Root {
        panels: panels.clone(),
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
        window.reset_layout_stats();
        window.refresh();
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
        let all = window.view_stats_all();
        assert_eq!(all.len(), panels.len() + 1);
        for (id, work) in all {
            assert!(
                work.built() > 0,
                "refresh must expose work in view {id}: {work:?}"
            );
        }
    })
    .unwrap();
}

#[test]
fn measurement_shaping_and_dirtiness_belong_to_the_leaf_view() {
    let mut cx = TestAppContext::single();
    let panel = cx.new(|_| Panel {
        label: "initial text".into(),
    });
    let window = cx.add_window(|window, _| {
        window.reset_layout_stats();
        Root {
            panels: vec![panel.clone()],
        }
    });
    cx.update_window(window.into(), |_, window, _| {
        let work = window.view_stats(panel.entity_id());
        assert!(
            work.nodes_created > 0 && work.measure_calls > 0 && work.lines_shaped > 0,
            "{work:?}"
        );
        assert_eq!(work.nodes_dirtied, 0, "creation is counted only once");
        assert_eq!(
            window
                .view_stats(window.root.as_ref().unwrap().entity_id())
                .measure_calls,
            0
        );
        window.reset_layout_stats();
    })
    .unwrap();
    panel.update(&mut cx, |panel, cx| {
        panel.label = "a much longer replacement text".into();
        cx.notify();
    });
    cx.update_window(window.into(), |_, window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
        let work = window.view_stats(panel.entity_id());
        assert!(
            work.nodes_reused > 0
                && work.nodes_dirtied > 0
                && work.measure_calls > 0
                && work.lines_shaped > 0,
            "{work:?}"
        );
        assert_eq!(work.nodes_created, 0);
        let global = window.layout_stats();
        assert_eq!(work.measure_calls, global.measure_calls);
        assert_eq!(work.lines_shaped, global.lines_shaped);
    })
    .unwrap();
}

#[test]
fn external_work_obeys_reset_and_nested_view_scopes() {
    let mut cx = TestAppContext::single();
    let window = cx.add_window(|_, _| Root { panels: Vec::new() });
    let first = EntityId::from((1_u64 << 32) | 900);
    let second = EntityId::from((3_u64 << 32) | 900);
    cx.update_window(window.into(), |_, window, _| {
        let work = ViewWork {
            nodes_created: 2,
            nodes_dirtied: 3,
            ..Default::default()
        };
        window.note_view_work(first, work);
        assert_eq!(window.view_stats(first), ViewWork::default());
        window.reset_layout_stats();
        window.with_rendered_view(first, |window| {
            let outer = window
                .text_system()
                .line_layout_cache
                .shaping
                .view_work
                .lock()
                .current;
            window.with_rendered_view(second, |window| {
                let inner = window
                    .text_system()
                    .line_layout_cache
                    .shaping
                    .view_work
                    .lock()
                    .current;
                assert_ne!(inner, outer);
                window.note_view_work(second, work);
            });
            let restored = window
                .text_system()
                .line_layout_cache
                .shaping
                .view_work
                .lock()
                .current;
            assert_eq!(restored, outer);
            window.note_view_work(first, work);
        });
        assert_eq!(window.view_stats(first).built(), 2);
        assert_eq!(window.view_stats(second).laid(), 5);
        assert_eq!(window.view_stats_all().len(), 3);
        assert_eq!(
            window
                .text_system()
                .line_layout_cache
                .shaping
                .view_work
                .lock()
                .current,
            None
        );
        window.reset_layout_stats();
        assert!(
            window
                .view_stats_all()
                .iter()
                .all(|(_, work)| *work == ViewWork::default())
        );
    })
    .unwrap();
}

fn instrumentation_cost(views: usize) -> (usize, usize) {
    let shared = Arc::new(parking_lot::Mutex::new(ViewCounters::default()));
    let mut engine = TaffyLayoutEngine::new();
    engine.retention.view_work = Some(shared.clone());
    let keys: Vec<Vec<_>> = (0..views)
        .map(|view| (0..64).map(|node| (view * 64 + node) as u64).collect())
        .collect();
    for (view, keys) in keys.iter().enumerate() {
        let mut counters = shared.lock();
        let slot = counters.bind(EntityId::from((1_u64 << 32) | (view + 1) as u64));
        counters.current = Some(slot);
        drop(counters);
        for key in keys {
            engine.request_layout(Some(*key), Style::default(), px(16.), 1., &[]);
        }
    }
    engine.clear();
    shared.lock().reset();
    shared.lock().steps = 0;
    ALLOCATIONS.with(|count| count.set(Some(0)));
    for (view, keys) in keys.iter().enumerate() {
        let mut counters = shared.lock();
        let slot = counters.bind(EntityId::from((1_u64 << 32) | (view + 1) as u64));
        counters.current = Some(slot);
        drop(counters);
        if view % 2 == 0 {
            for key in keys {
                engine.request_layout(Some(*key), Style::default(), px(16.), 1., &[]);
            }
        } else {
            engine.keep_retained(keys);
        }
    }
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    let steps = shared.lock().steps;
    (steps, allocations)
}

#[test]
fn counter_work_scales_linearly_without_per_node_allocations() {
    let (small, small_allocations) = instrumentation_cost(60);
    let (large, large_allocations) = instrumentation_cost(240);
    assert!(small > 0);
    assert_eq!(
        large,
        4 * small,
        "one bind per view and one indexed count per node"
    );
    assert_eq!(
        small_allocations, 0,
        "allocation budget at 60 views x 64 nodes"
    );
    assert_eq!(
        large_allocations, 0,
        "allocation budget at 240 views x 64 nodes"
    );
}
