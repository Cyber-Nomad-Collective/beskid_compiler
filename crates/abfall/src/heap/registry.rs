use crate::gc_box::GcHeader;
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Ownership index for opaque Beskid allocations.
///
/// GC headers own allocation lifetime; generated code only receives payload pointers. Keeping
/// the two directions together makes pointer-map tracing and sweep cleanup share one authority.
#[derive(Default)]
pub(super) struct BeskidAllocationRegistry {
    mappings: parking_lot::Mutex<BeskidAllocationMappings>,
}

#[derive(Default)]
struct BeskidAllocationMappings {
    payload_to_header: BTreeMap<usize, AllocationRange>,
    header_to_payload: BTreeMap<usize, usize>,
    construction_roots: HashSet<usize>,
    composite_children: BTreeMap<usize, BTreeSet<usize>>,
}

#[derive(Clone, Copy)]
struct AllocationRange {
    header: usize,
    end_exclusive: usize,
}

impl BeskidAllocationRegistry {
    pub(super) fn register(
        &self,
        payload: *mut u8,
        payload_len: usize,
        header: *mut GcHeader,
        has_construction_root: bool,
    ) {
        let Some(end_exclusive) = (payload as usize).checked_add(payload_len) else {
            return;
        };
        let mut mappings = self.mappings.lock();
        mappings.payload_to_header.insert(payload as usize, AllocationRange { header: header as usize, end_exclusive });
        mappings.header_to_payload.insert(header as usize, payload as usize);
        if has_construction_root {
            mappings.construction_roots.insert(payload as usize);
        }
    }

    pub(super) fn unregister(&self, header: *mut GcHeader) {
        let mut mappings = self.mappings.lock();
        if let Some(payload) = mappings.header_to_payload.remove(&(header as usize)) {
            mappings.payload_to_header.remove(&payload);
            mappings.construction_roots.remove(&payload);
            mappings.composite_children.remove(&payload);
            for children in mappings.composite_children.values_mut() {
                children.remove(&payload);
            }
        }
    }

    fn owner_payload(mappings: &BeskidAllocationMappings, payload: *mut u8) -> Option<usize> {
        let address = payload as usize;
        mappings
            .payload_to_header
            .range(..=address)
            .next_back()
            .and_then(|(base, range)| (address < range.end_exclusive).then_some(*base))
    }

    pub(super) fn header_for(&self, payload: *mut u8) -> Option<*mut GcHeader> {
        let mappings = self.mappings.lock();
        Self::owner_payload(&mappings, payload)
            .and_then(|base| mappings.payload_to_header.get(&base))
            .map(|range| range.header as *mut GcHeader)
    }

    pub(super) fn owns(&self, payload: *mut u8) -> bool {
        !payload.is_null() && self.header_for(payload).is_some()
    }

    pub(super) fn take_construction_root(&self, payload: *mut u8) -> Option<*mut GcHeader> {
        let mut mappings = self.mappings.lock();
        if mappings.construction_roots.remove(&(payload as usize)) {
            mappings.payload_to_header.get(&(payload as usize)).map(|range| range.header as *mut GcHeader)
        } else {
            None
        }
    }

    pub(super) fn add_composite_edge(&self, parent: *mut u8, child: *mut u8) {
        if parent.is_null() || child.is_null() {
            return;
        }
        let mut mappings = self.mappings.lock();
        if mappings.payload_to_header.contains_key(&(parent as usize))
            && let Some(child_owner) = Self::owner_payload(&mappings, child)
        {
            mappings.composite_children.entry(parent as usize).or_default().insert(child_owner);
        }
    }

    pub(super) fn composite_children(&self, parent: *mut u8) -> Vec<*mut u8> {
        self.mappings
            .lock()
            .composite_children
            .get(&(parent as usize))
            .into_iter()
            .flatten()
            .copied()
            .map(|child| child as *mut u8)
            .collect()
    }
}
