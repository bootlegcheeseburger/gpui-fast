//! Fork patch: per-view work attribution for test-support consumers.

use crate::{App, AvailableSpace, EntityId, Pixels, Size, Window, taffy::MeasureFn};

#[cfg(any(test, feature = "test-support"))]
use crate::{LayoutId, fast::layout::LayoutRetention, taffy::TaffyLayoutEngine};
#[cfg(any(test, feature = "test-support"))]
use collections::FxHashMap;
#[cfg(any(test, feature = "test-support"))]
use parking_lot::Mutex;
#[cfg(any(test, feature = "test-support"))]
use std::sync::Arc;

/// Fork patch: work attributed exclusively to one view, accumulated until reset.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewWork {
    /// Fork patch: layout nodes allocated by the view.
    pub nodes_created: u64,
    /// Fork patch: existing nodes claimed by elements built again.
    pub nodes_reused: u64,
    /// Fork patch: existing nodes kept without building their elements.
    pub nodes_kept: u64,
    /// Fork patch: measurement closure calls, including replay and settling.
    pub measure_calls: u64,
    /// Fork patch: lines shaped on a cache miss, as in LayoutStats.
    pub lines_shaped: u64,
    /// Fork patch: existing nodes dirtied, once per node per frame.
    pub nodes_dirtied: u64,
}

#[cfg(any(test, feature = "test-support"))]
impl ViewWork {
    /// Fork patch: nodes requested by elements built again; excludes kept nodes.
    pub fn built(self) -> u64 {
        self.nodes_created + self.nodes_reused
    }

    /// Fork patch: new nodes plus existing nodes dirtied, rather than solver visits.
    pub fn laid(self) -> u64 {
        self.nodes_created + self.nodes_dirtied
    }

    fn add(&mut self, other: Self) {
        self.nodes_created += other.nodes_created;
        self.nodes_reused += other.nodes_reused;
        self.nodes_kept += other.nodes_kept;
        self.measure_calls += other.measure_calls;
        self.lines_shaped += other.lines_shaped;
        self.nodes_dirtied += other.nodes_dirtied;
    }
}

// Fork patch: resolved once at a view boundary, copied on nodes, never a string.
#[cfg(any(test, feature = "test-support"))]
pub(crate) type ViewSlot = Option<usize>;

// Fork patch: one shared table per window, including while its engine is taken out.
#[cfg(any(test, feature = "test-support"))]
pub(crate) type SharedWork = Arc<Mutex<ViewCounters>>;

#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
pub(crate) struct ViewCounters {
    slots: FxHashMap<EntityId, usize>,
    entries: Vec<(EntityId, ViewWork)>,
    pub(crate) current: ViewSlot,
    enabled: bool,
    #[cfg(test)]
    pub(crate) steps: usize,
}

#[cfg(any(test, feature = "test-support"))]
impl ViewCounters {
    pub(crate) fn bind(&mut self, id: EntityId) -> usize {
        #[cfg(test)]
        {
            self.steps += 1;
        }
        *self.slots.entry(id).or_insert_with(|| {
            let slot = self.entries.len();
            self.entries.push((id, ViewWork::default()));
            slot
        })
    }

    pub(crate) fn note(&mut self, owner: ViewSlot, work: ViewWork) {
        #[cfg(test)]
        {
            self.steps += 1;
        }
        if self.enabled
            && let Some(owner) = owner
        {
            self.entries[owner].1.add(work);
        }
    }

    pub(crate) fn unkeep(&mut self, owner: ViewSlot) {
        if self.enabled
            && let Some(owner) = owner
        {
            self.entries[owner].1.nodes_kept = self.entries[owner].1.nodes_kept.saturating_sub(1);
        }
    }

    pub(crate) fn reset(&mut self) {
        for (_, work) in &mut self.entries {
            *work = ViewWork::default();
        }
        self.enabled = true;
    }
}

// Fork patch: stored by node id for constant-time attribution outside a key claim.
#[cfg(any(test, feature = "test-support"))]
#[derive(Default, Clone, Copy)]
pub(crate) struct NodeWork {
    pub(crate) owner: ViewSlot,
    created: u64,
    dirtied: Option<u64>,
    pub(crate) claimed: bool,
}

#[cfg(any(test, feature = "test-support"))]
impl LayoutRetention {
    pub(crate) fn work_owner(&self) -> ViewSlot {
        self.view_work.as_ref().and_then(|work| work.lock().current)
    }

    pub(crate) fn note_work(&self, owner: ViewSlot, work: ViewWork) {
        if let Some(counters) = &self.view_work {
            counters.lock().note(owner, work);
        }
    }

    pub(crate) fn created_work(&mut self, id: LayoutId) {
        let owner = self.work_owner();
        self.node_work.insert(
            id,
            NodeWork {
                owner,
                created: self.frame,
                dirtied: None,
                claimed: false,
            },
        );
        self.note_work(
            owner,
            ViewWork {
                nodes_created: 1,
                ..ViewWork::default()
            },
        );
    }

    pub(crate) fn dirty_work(&mut self, id: LayoutId) {
        let Some(mut node) = self.node_work.get(&id).copied() else {
            return;
        };
        if node.created == self.frame || node.dirtied == Some(self.frame) {
            return;
        }
        node.dirtied = Some(self.frame);
        self.node_work.insert(id, node);
        self.note_work(
            node.owner,
            ViewWork {
                nodes_dirtied: 1,
                ..ViewWork::default()
            },
        );
    }
}

// Fork patch: the upstream boundary still maintains the rendering stack.
pub(crate) fn with_rendered_view<R>(
    window: &mut Window,
    id: EntityId,
    f: impl FnOnce(&mut Window) -> R,
) -> R {
    #[cfg(any(test, feature = "test-support"))]
    let previous = {
        let shared = &window.text_system.line_layout_cache.shaping.view_work;
        if let Some(engine) = window.layout_engine.as_mut() {
            engine
                .retention
                .view_work
                .get_or_insert_with(|| shared.clone());
        }
        let mut counters = shared.lock();
        let slot = counters.bind(id);
        counters.current.replace(slot)
    };
    window.rendered_entity_stack.push(id);
    let result = f(window);
    window.rendered_entity_stack.pop();
    #[cfg(any(test, feature = "test-support"))]
    {
        window
            .text_system()
            .line_layout_cache
            .shaping
            .view_work
            .lock()
            .current = previous;
    }
    result
}

// Fork patch: measures run under their node's owner, including text cache misses.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(crate) fn measure(
    retention: &crate::fast::layout::LayoutRetention,
    id: crate::LayoutId,
    measure: &mut MeasureFn,
    known: Size<Option<Pixels>>,
    available: Size<AvailableSpace>,
    window: &mut Window,
    cx: &mut App,
) -> Size<Pixels> {
    #[cfg(any(test, feature = "test-support"))]
    let previous = {
        let mut counters = window
            .text_system()
            .line_layout_cache
            .shaping
            .view_work
            .lock();
        let owner = retention.node_work.get(&id).and_then(|node| node.owner);
        counters.note(
            owner,
            ViewWork {
                measure_calls: 1,
                ..ViewWork::default()
            },
        );
        std::mem::replace(&mut counters.current, owner)
    };
    #[cfg(not(any(test, feature = "test-support")))]
    let _ = (retention, id);
    let size = measure(known, available, window, cx);
    #[cfg(any(test, feature = "test-support"))]
    {
        window
            .text_system()
            .line_layout_cache
            .shaping
            .view_work
            .lock()
            .current = previous;
    }
    size
}

#[cfg(any(test, feature = "test-support"))]
impl Window {
    /// Fork patch: work for a view since reset_layout_stats, or zero if unseen.
    pub fn view_stats(&self, id: EntityId) -> ViewWork {
        let counters = self
            .text_system()
            .line_layout_cache
            .shaping
            .view_work
            .lock();
        counters
            .slots
            .get(&id)
            .map_or(ViewWork::default(), |slot| counters.entries[*slot].1)
    }

    /// Fork patch: all known views, including idle views whose counters are zero.
    pub fn view_stats_all(&self) -> Vec<(EntityId, ViewWork)> {
        self.text_system()
            .line_layout_cache
            .shaping
            .view_work
            .lock()
            .entries
            .clone()
    }

    /// Fork patch: report work by an external renderer using the same reset interval.
    pub fn note_view_work(&mut self, id: EntityId, work: ViewWork) {
        let mut counters = self
            .text_system()
            .line_layout_cache
            .shaping
            .view_work
            .lock();
        let slot = counters.bind(id);
        counters.note(Some(slot), work);
    }
}

// Fork patch: a cached view's unchanged wrapper was claimed but its content kept.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn cached_wrapper_kept(engine: &mut TaffyLayoutEngine, keys: &[u64]) {
    for key in keys {
        if let Some(node) = engine.retention.retained.get(key) {
            let Some(mut counted) = engine.retention.node_work.get(&node.id).copied() else {
                continue;
            };
            if !counted.claimed {
                continue;
            }
            counted.claimed = false;
            engine.retention.node_work.insert(node.id, counted);
            let Some(shared) = &engine.retention.view_work else {
                return;
            };
            let mut counters = shared.lock();
            if counters.enabled
                && let Some(owner) = node.owner
            {
                let work = &mut counters.entries[owner].1;
                work.nodes_reused = work.nodes_reused.saturating_sub(1);
                work.nodes_kept += 1;
            }
        }
    }
}
