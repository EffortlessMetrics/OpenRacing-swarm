//! Generic lock-free seqlock-style mailbox for copy types.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU32, Ordering};

/// Lock-free, single-writer/multi-reader mailbox.
///
/// The writer increments a sequence counter, writes payload, and publishes an
/// even sequence value when the snapshot is complete.
pub struct SnapshotMailbox<T: Copy> {
    seq: AtomicU32,
    data: UnsafeCell<T>,
}

// SAFETY: `SnapshotMailbox` exposes mutation only through `write`, which is a
// single-writer API by construction for this engine surface. Readers copy `T`
// through the seqlock retry loop and accept a value only when the sequence is
// even and unchanged across the copy.
unsafe impl<T: Copy> Sync for SnapshotMailbox<T> {}

impl<T: Copy> SnapshotMailbox<T> {
    pub const fn new(value: T) -> Self {
        Self {
            seq: AtomicU32::new(0),
            data: UnsafeCell::new(value),
        }
    }

    pub fn write(&self, value: T) {
        self.seq.fetch_add(1, Ordering::Release);
        // SAFETY: The engine owns a single writer for each mailbox. Publishing
        // an odd sequence before the write prevents readers from accepting a
        // concurrently copied value.
        unsafe {
            *self.data.get() = value;
        }
        self.seq.fetch_add(1, Ordering::Release);
    }

    pub fn read(&self) -> T {
        loop {
            let start = self.seq.load(Ordering::Acquire);
            if (start & 1) != 0 {
                continue;
            }

            // SAFETY: `T: Copy`; if this races with the single writer, the
            // sequence comparison below rejects the copied value and retries.
            let value = unsafe { *self.data.get() };
            let end = self.seq.load(Ordering::Acquire);
            if start == end {
                return value;
            }
        }
    }
}
