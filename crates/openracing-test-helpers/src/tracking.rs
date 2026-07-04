//! Allocation tracking for RT safety tests.
//!
//! This module provides utilities to verify that code paths don't allocate
//! on the heap, which is critical for real-time safety.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static ALLOCATION_COUNT: Cell<usize> = const { Cell::new(0) };
    static ALLOCATION_BYTES: Cell<usize> = const { Cell::new(0) };
    static TRACKING_ENABLED: Cell<bool> = const { Cell::new(false) };
}

/// Test-only allocator wrapper that delegates allocation to [`System`] and
/// records successful allocations in thread-local counters when tracking is
/// enabled.
///
/// # Safety contract
///
/// `TrackingAllocator` does not alter allocator ownership or layout rules. Each
/// allocation primitive forwards the caller-provided pointer and [`Layout`] to
/// [`System`] under the same [`GlobalAlloc`] contract, and then records only
/// successful allocation/growth results without dereferencing allocator
/// pointers.
pub struct TrackingAllocator;

// SAFETY: `TrackingAllocator` preserves the `GlobalAlloc` contract by
// delegating every allocation primitive to `System` with the original
// caller-provided pointer/layout arguments. Its local side effect is limited to
// thread-local accounting after successful allocations, guarded by non-null
// result checks and explicit tracking enablement.
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller of `GlobalAlloc::alloc` must provide a valid
        // `Layout`. This implementation forwards that layout unchanged to the
        // system allocator and does not dereference the returned pointer.
        let ptr = unsafe { System.alloc(layout) };
        record_allocation(ptr, layout);
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller of `GlobalAlloc::dealloc` must provide a pointer
        // previously allocated by this allocator with the matching `Layout`.
        // The tracking wrapper forwards those arguments unchanged and performs
        // no pointer access of its own.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: The caller of `GlobalAlloc::realloc` must provide a pointer
        // and layout that satisfy the allocator contract. This wrapper delegates
        // the operation to `System`, then records only successful growth.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            record_reallocation_growth(layout, new_size);
        }
        new_ptr
    }
}

fn tracking_enabled() -> bool {
    TRACKING_ENABLED.with(Cell::get)
}

fn set_tracking_enabled(enabled: bool) {
    TRACKING_ENABLED.with(|enabled_cell| enabled_cell.set(enabled));
}

fn allocation_count() -> usize {
    ALLOCATION_COUNT.with(Cell::get)
}

fn allocation_bytes() -> usize {
    ALLOCATION_BYTES.with(Cell::get)
}

fn reset_tracking_counters() {
    ALLOCATION_COUNT.with(|count| count.set(0));
    ALLOCATION_BYTES.with(|bytes| bytes.set(0));
}

fn add_allocation_count(delta: usize) {
    ALLOCATION_COUNT.with(|count| {
        count.set(count.get().saturating_add(delta));
    });
}

fn add_allocation_bytes(delta: usize) {
    ALLOCATION_BYTES.with(|bytes| {
        bytes.set(bytes.get().saturating_add(delta));
    });
}

fn record_allocation(ptr: *mut u8, layout: Layout) {
    if ptr.is_null() || !tracking_enabled() {
        return;
    }

    add_allocation_count(1);
    add_allocation_bytes(layout.size());
}

fn record_reallocation_growth(layout: Layout, new_size: usize) {
    if !tracking_enabled() || new_size <= layout.size() {
        return;
    }

    add_allocation_bytes(new_size - layout.size());
}

pub struct AllocationGuard {
    start_count: usize,
    start_bytes: usize,
    _private: (),
}

impl AllocationGuard {
    pub fn new() -> Self {
        set_tracking_enabled(true);
        Self {
            start_count: allocation_count(),
            start_bytes: allocation_bytes(),
            _private: (),
        }
    }

    pub fn allocations(&self) -> usize {
        allocation_count().saturating_sub(self.start_count)
    }

    pub fn bytes(&self) -> usize {
        allocation_bytes().saturating_sub(self.start_bytes)
    }

    pub fn has_allocations(&self) -> bool {
        self.allocations() > 0
    }

    pub fn reset(&self) {
        reset_tracking_counters();
    }
}

impl Default for AllocationGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AllocationGuard {
    fn drop(&mut self) {
        set_tracking_enabled(false);
    }
}

pub fn track() -> AllocationGuard {
    AllocationGuard::new()
}

#[macro_export]
macro_rules! assert_rt_safe {
    ($guard:expr) => {
        let _guard = &$guard;
        let allocs = _guard.allocations();
        let bytes = _guard.bytes();
        if allocs > 0 {
            panic!(
                "RT path allocation violation: {} allocations ({} bytes)\n\
                 This violates the zero-allocation requirement for real-time code.\n\
                 Location: {}:{}",
                allocs,
                bytes,
                file!(),
                line!()
            );
        }
    };
    ($guard:expr, $context:expr) => {
        let _guard = &$guard;
        let allocs = _guard.allocations();
        let bytes = _guard.bytes();
        if allocs > 0 {
            panic!(
                "RT path allocation violation in '{}': {} allocations ({} bytes)\n\
                 This violates the zero-allocation requirement for real-time code.\n\
                 Location: {}:{}",
                $context,
                allocs,
                bytes,
                file!(),
                line!()
            );
        }
    };
}

#[macro_export]
macro_rules! ci_assert_rt_safe {
    ($guard:expr, $context:expr) => {
        let _guard = &$guard;
        let allocs = _guard.allocations();
        let bytes = _guard.bytes();
        if allocs > 0 {
            eprintln!("CI FAILURE: RT path allocation detected in '{}'", $context);
            eprintln!("  Allocations: {}", allocs);
            eprintln!("  Bytes: {}", bytes);
            eprintln!("This violates the zero-allocation requirement for real-time code.");
            std::process::exit(1);
        }
    };
}

pub struct AllocationReport {
    pub allocations: usize,
    pub bytes: usize,
    pub context: String,
}

impl AllocationReport {
    pub fn new(context: impl Into<String>) -> Self {
        Self {
            allocations: 0,
            bytes: 0,
            context: context.into(),
        }
    }

    pub fn assert_zero(&self) -> &Self {
        if self.allocations > 0 {
            panic!(
                "Allocation violation in '{}': {} allocations ({} bytes)",
                self.context, self.allocations, self.bytes
            );
        }
        self
    }

    pub fn is_zero(&self) -> bool {
        self.allocations == 0
    }
}

impl std::fmt::Display for AllocationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.allocations > 0 {
            write!(
                f,
                "⚠️  {} allocated {} times ({} bytes)",
                self.context, self.allocations, self.bytes
            )
        } else {
            write!(f, "✅ {} - zero allocations", self.context)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::NonNull;

    fn test_layout_8() -> Layout {
        Layout::new::<u64>()
    }

    fn test_layout_16() -> Layout {
        Layout::new::<[u64; 2]>()
    }

    fn test_layout_64() -> Layout {
        Layout::new::<[u64; 8]>()
    }

    #[test]
    fn test_guard_no_allocations() {
        let guard = track();
        let x = 42;
        let _y = x + 1;
        assert_rt_safe!(guard);
    }

    #[test]
    #[should_panic(expected = "RT path allocation violation")]
    fn test_guard_with_allocations() {
        let guard = track();
        let _vec: Vec<i32> = vec![1, 2, 3];
        assert_rt_safe!(guard);
    }

    #[test]
    fn test_guard_allocations_count() {
        let guard = track();
        let _vec: Vec<i32> = vec![1, 2, 3, 4, 5];
        assert!(guard.allocations() > 0);
        assert!(guard.bytes() > 0);
    }

    #[test]
    fn test_guard_has_allocations() {
        let guard = track();
        assert!(!guard.has_allocations());
        let _vec: Vec<i32> = vec![1, 2, 3];
        assert!(guard.has_allocations());
    }

    #[test]
    fn test_allocation_report() {
        let report = AllocationReport::new("test");
        assert!(report.is_zero());
        report.assert_zero();
    }

    #[test]
    #[should_panic(expected = "Allocation violation")]
    fn test_allocation_report_assert() {
        let report = AllocationReport {
            allocations: 1,
            bytes: 100,
            context: "test".to_string(),
        };
        report.assert_zero();
    }

    #[test]
    fn test_allocation_report_display() {
        let zero = AllocationReport::new("zero");
        assert!(zero.to_string().contains("zero allocations"));

        let nonzero = AllocationReport {
            allocations: 3,
            bytes: 256,
            context: "nonzero".to_string(),
        };
        let s = nonzero.to_string();
        assert!(s.contains("3 times"));
        assert!(s.contains("256 bytes"));
    }

    #[test]
    fn test_nested_guards() {
        let guard1 = track();
        let guard2 = track();
        let _x = 1;
        assert_rt_safe!(guard2);
        assert_rt_safe!(guard1);
    }

    #[test]
    fn record_allocation_requires_enabled_tracking_and_non_null_pointer() {
        reset_tracking_counters();
        set_tracking_enabled(false);

        let ptr = NonNull::<u8>::dangling().as_ptr();
        record_allocation(ptr, test_layout_16());

        assert_eq!(allocation_count(), 0);
        assert_eq!(allocation_bytes(), 0);

        set_tracking_enabled(true);
        record_allocation(std::ptr::null_mut::<u8>(), test_layout_16());

        assert_eq!(allocation_count(), 0);
        assert_eq!(allocation_bytes(), 0);

        record_allocation(ptr, test_layout_16());

        assert_eq!(allocation_count(), 1);
        assert_eq!(allocation_bytes(), 16);
        set_tracking_enabled(false);
    }

    #[test]
    fn record_reallocation_growth_counts_only_successful_growth() {
        reset_tracking_counters();
        set_tracking_enabled(true);

        record_reallocation_growth(test_layout_64(), 32);
        record_reallocation_growth(test_layout_64(), 64);

        assert_eq!(allocation_bytes(), 0);

        record_reallocation_growth(test_layout_64(), 96);

        assert_eq!(allocation_bytes(), 32);
        set_tracking_enabled(false);
    }

    #[test]
    fn allocation_tracking_saturates_counters() {
        reset_tracking_counters();
        set_tracking_enabled(true);

        ALLOCATION_COUNT.with(|count| count.set(usize::MAX));
        ALLOCATION_BYTES.with(|bytes| bytes.set(usize::MAX - 1));

        let ptr = NonNull::<u8>::dangling().as_ptr();
        record_allocation(ptr, test_layout_8());

        assert_eq!(allocation_count(), usize::MAX);
        assert_eq!(allocation_bytes(), usize::MAX);
        set_tracking_enabled(false);
    }
}
