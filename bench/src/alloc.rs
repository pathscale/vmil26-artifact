//! A counting global allocator.
//!
//! `blitz-wasm` already reports its own allocations, and its ABI.md explains at
//! length why that number is not the whole truth: it counts what that crate
//! does while servicing a call and cannot see into `blitz-dom-api` or
//! `blitz-dom` underneath. The paper this harness feeds makes the same point
//! more sharply, about an earlier revision of that very counter, which read
//! zero while two `String`s were allocated per attribute write.
//!
//! The lesson taken from that is not "count more carefully". It is that an
//! instrument which records what its author chose to record will agree with its
//! author. So the allocation figure here is taken below every crate involved,
//! where nothing can opt out of being counted: arm A's Boa strings, arm C's
//! wasmi bookkeeping, and every `String` either engine builds on the way to the
//! document all land in the same counter.
//!
//! The trade is the opposite failure: this catches allocation that has nothing
//! to do with the boundary. Both figures are therefore reported side by side.
//! Neither is the honest one alone.
//!
//! # What it costs
//!
//! Two relaxed atomic adds on allocation and one on deallocation. Arm A and
//! arm C pay it identically, and it stays installed during the timing pass
//! rather than being compiled out, because a timing pass measuring different
//! code from the counting pass would be measuring a different program.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);

/// Wraps the system allocator and counts.
pub struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    /// Counted as one allocation of the new size.
    ///
    /// Not delegated to the default `alloc`-copy-`dealloc`, because `System`
    /// can often grow a block in place and forcing the copy would make the
    /// harness slower than the program it is measuring. A `Vec` that doubles
    /// its way to 10,000 rows therefore reports its handful of reallocations
    /// rather than being invisible.
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }
}

/// A reading of the counters, taken at a point in time.
#[derive(Debug, Clone, Copy, Default)]
pub struct Snapshot {
    pub allocations: u64,
    pub allocated_bytes: u64,
    pub deallocations: u64,
}

impl Snapshot {
    /// Read the counters now.
    #[must_use]
    pub fn take() -> Self {
        Self {
            allocations: ALLOCATIONS.load(Ordering::Relaxed),
            allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
            deallocations: DEALLOCATIONS.load(Ordering::Relaxed),
        }
    }

    /// What happened between `self` and now.
    ///
    /// Saturating, so a counter that somehow went backwards reports zero
    /// rather than a number near `u64::MAX` that would look like a finding.
    #[must_use]
    pub fn since(self) -> Self {
        let now = Self::take();
        Self {
            allocations: now.allocations.saturating_sub(self.allocations),
            allocated_bytes: now.allocated_bytes.saturating_sub(self.allocated_bytes),
            deallocations: now.deallocations.saturating_sub(self.deallocations),
        }
    }
}
