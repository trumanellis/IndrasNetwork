//! # Low Memory Mode
//!
//! Memory-constrained operation for embedded systems with limited RAM.
//! Provides buffer pooling, streaming operations, and memory limits.
//!
//! ## Thread Safety
//!
//! [`MemoryTracker`] is thread-safe and can be shared across threads via `Arc<MemoryTracker>`.
//! All resource tracking uses atomic compare-and-swap operations to prevent race conditions.
//!
//! ## Example
//!
//! ```
//! use indras_iot::low_memory::{MemoryTracker, MemoryBudget};
//! use std::sync::Arc;
//!
//! let tracker = Arc::new(MemoryTracker::new(MemoryBudget::default()));
//!
//! // Allocate memory with RAII guard
//! let guard = tracker.try_allocate(1024).unwrap();
//! assert_eq!(tracker.allocated_bytes(), 1024);
//!
//! // Memory automatically freed when guard drops
//! drop(guard);
//! assert_eq!(tracker.allocated_bytes(), 0);
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};
use thiserror::Error;

/// Memory budget configuration
#[derive(Debug, Clone)]
pub struct MemoryBudget {
    /// Maximum heap allocation in bytes
    pub max_heap_bytes: usize,
    /// Maximum message buffer size
    pub max_message_size: usize,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Maximum pending operations
    pub max_pending_ops: usize,
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self {
            max_heap_bytes: 64 * 1024, // 64KB default
            max_message_size: 1024,    // 1KB messages
            max_connections: 4,
            max_pending_ops: 8,
        }
    }
}

impl MemoryBudget {
    /// Create a minimal budget for very constrained devices
    pub fn minimal() -> Self {
        Self {
            max_heap_bytes: 16 * 1024, // 16KB
            max_message_size: 256,
            max_connections: 2,
            max_pending_ops: 4,
        }
    }

    /// Create a budget for moderate IoT devices
    pub fn moderate() -> Self {
        Self {
            max_heap_bytes: 128 * 1024, // 128KB
            max_message_size: 4096,
            max_connections: 8,
            max_pending_ops: 16,
        }
    }
}

/// Errors for memory-constrained operations
#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Memory budget exceeded: requested {requested} bytes, available {available}")]
    BudgetExceeded { requested: usize, available: usize },
    #[error("Message too large: {size} bytes exceeds limit of {limit}")]
    MessageTooLarge { size: usize, limit: usize },
    #[error("Connection limit reached: {current}/{max}")]
    ConnectionLimitReached { current: usize, max: usize },
    #[error("Operation queue full: {current}/{max}")]
    OperationQueueFull { current: usize, max: usize },
    #[error("No buffers available in pool")]
    NoBuffersAvailable,
}

/// Memory tracker for enforcing budgets.
///
/// This type is thread-safe and uses atomic compare-and-swap operations
/// to prevent race conditions when multiple threads allocate concurrently.
#[derive(Debug)]
pub struct MemoryTracker {
    budget: MemoryBudget,
    allocated: AtomicUsize,
    connections: AtomicUsize,
    pending_ops: AtomicUsize,
}

impl MemoryTracker {
    /// Create a new tracker with the given budget
    pub fn new(budget: MemoryBudget) -> Self {
        Self {
            budget,
            allocated: AtomicUsize::new(0),
            connections: AtomicUsize::new(0),
            pending_ops: AtomicUsize::new(0),
        }
    }

    /// Try to allocate memory using atomic compare-and-swap.
    ///
    /// Returns a guard that automatically frees the memory when dropped.
    /// This operation is thread-safe and will not exceed the budget even
    /// under concurrent access.
    pub fn try_allocate(&self, bytes: usize) -> Result<MemoryGuard<'_>, MemoryError> {
        loop {
            let current = self.allocated.load(Ordering::Acquire);

            // Check for overflow and budget
            let new_value = match current.checked_add(bytes) {
                Some(v) if v <= self.budget.max_heap_bytes => v,
                _ => {
                    return Err(MemoryError::BudgetExceeded {
                        requested: bytes,
                        available: self.budget.max_heap_bytes.saturating_sub(current),
                    });
                }
            };

            // Atomic compare-and-swap
            match self.allocated.compare_exchange_weak(
                current,
                new_value,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Ok(MemoryGuard {
                        tracker: self,
                        bytes,
                    });
                }
                Err(_) => {
                    // CAS failed, another thread modified the value, retry
                    continue;
                }
            }
        }
    }

    /// Check if a message size is allowed
    pub fn check_message_size(&self, size: usize) -> Result<(), MemoryError> {
        if size > self.budget.max_message_size {
            return Err(MemoryError::MessageTooLarge {
                size,
                limit: self.budget.max_message_size,
            });
        }
        Ok(())
    }

    /// Try to add a connection using atomic compare-and-swap.
    ///
    /// Returns a guard that automatically releases the connection slot when dropped.
    pub fn try_add_connection(&self) -> Result<ConnectionGuard<'_>, MemoryError> {
        loop {
            let current = self.connections.load(Ordering::Acquire);

            if current >= self.budget.max_connections {
                return Err(MemoryError::ConnectionLimitReached {
                    current,
                    max: self.budget.max_connections,
                });
            }

            match self.connections.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Ok(ConnectionGuard { tracker: self });
                }
                Err(_) => continue,
            }
        }
    }

    /// Try to queue an operation using atomic compare-and-swap.
    ///
    /// Returns a guard that automatically releases the operation slot when dropped.
    pub fn try_queue_op(&self) -> Result<OpGuard<'_>, MemoryError> {
        loop {
            let current = self.pending_ops.load(Ordering::Acquire);

            if current >= self.budget.max_pending_ops {
                return Err(MemoryError::OperationQueueFull {
                    current,
                    max: self.budget.max_pending_ops,
                });
            }

            match self.pending_ops.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Ok(OpGuard { tracker: self });
                }
                Err(_) => continue,
            }
        }
    }

    /// Get current memory usage
    pub fn allocated_bytes(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
    }

    /// Get available memory
    pub fn available_bytes(&self) -> usize {
        self.budget
            .max_heap_bytes
            .saturating_sub(self.allocated.load(Ordering::Relaxed))
    }

    /// Get current connection count
    pub fn connection_count(&self) -> usize {
        self.connections.load(Ordering::Relaxed)
    }

    /// Get current pending ops count
    pub fn pending_ops_count(&self) -> usize {
        self.pending_ops.load(Ordering::Relaxed)
    }

    /// Get the budget
    pub fn budget(&self) -> &MemoryBudget {
        &self.budget
    }
}

/// RAII guard for memory allocation.
///
/// When this guard is dropped, the allocated memory is automatically freed.
#[derive(Debug)]
pub struct MemoryGuard<'a> {
    tracker: &'a MemoryTracker,
    bytes: usize,
}

impl MemoryGuard<'_> {
    /// Get the number of bytes held by this guard
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for MemoryGuard<'_> {
    fn drop(&mut self) {
        let prev = self
            .tracker
            .allocated
            .fetch_sub(self.bytes, Ordering::AcqRel);
        debug_assert!(
            prev >= self.bytes,
            "MemoryGuard underflow: had {}, subtracting {}",
            prev,
            self.bytes
        );
    }
}

/// RAII guard for connection.
///
/// When this guard is dropped, the connection slot is automatically released.
#[derive(Debug)]
pub struct ConnectionGuard<'a> {
    tracker: &'a MemoryTracker,
}

impl Drop for ConnectionGuard<'_> {
    fn drop(&mut self) {
        let prev = self.tracker.connections.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(
            prev >= 1,
            "ConnectionGuard underflow: had {}, subtracting 1",
            prev
        );
    }
}

/// RAII guard for pending operation.
///
/// When this guard is dropped, the operation slot is automatically released.
#[derive(Debug)]
pub struct OpGuard<'a> {
    tracker: &'a MemoryTracker,
}

impl Drop for OpGuard<'_> {
    fn drop(&mut self) {
        let prev = self.tracker.pending_ops.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(prev >= 1, "OpGuard underflow: had {}, subtracting 1", prev);
    }
}

/// A fixed-size buffer pool for zero-allocation message handling.
///
/// Pre-allocates a set of fixed-size buffers that can be acquired and released
/// without heap allocation during normal operation.
pub struct BufferPool {
    buffers: Vec<Option<Vec<u8>>>,
    buffer_size: usize,
    available: AtomicUsize,
}

impl BufferPool {
    /// Create a new buffer pool with `count` buffers of `buffer_size` bytes each.
    ///
    /// # Panics
    ///
    /// Panics if `buffer_size` is zero.
    pub fn new(count: usize, buffer_size: usize) -> Self {
        assert!(buffer_size > 0, "buffer_size must be positive");
        let buffers = (0..count).map(|_| Some(vec![0u8; buffer_size])).collect();
        Self {
            buffers,
            buffer_size,
            available: AtomicUsize::new(count),
        }
    }

    /// Try to acquire a buffer from the pool.
    ///
    /// Returns `None` if no buffers are available.
    pub fn try_acquire(&mut self) -> Option<PooledBuffer> {
        for (index, slot) in self.buffers.iter_mut().enumerate() {
            if let Some(buffer) = slot.take() {
                self.available.fetch_sub(1, Ordering::AcqRel);
                return Some(PooledBuffer { buffer, index });
            }
        }
        None
    }

    /// Release a buffer back to the pool.
    ///
    /// # Panics
    ///
    /// Panics if the buffer's index is out of bounds or the slot is already occupied.
    pub fn release(&mut self, pooled: PooledBuffer) {
        assert!(pooled.index < self.buffers.len(), "invalid buffer index");
        assert!(
            self.buffers[pooled.index].is_none(),
            "buffer slot already occupied"
        );

        let mut buffer = pooled.buffer;
        // Clear the buffer for security
        buffer.fill(0);
        // Resize back to original size in case it was modified
        buffer.resize(self.buffer_size, 0);

        self.buffers[pooled.index] = Some(buffer);
        self.available.fetch_add(1, Ordering::AcqRel);
    }

    /// Get the buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// Get the total number of buffers in the pool
    pub fn capacity(&self) -> usize {
        self.buffers.len()
    }

    /// Get the number of buffers currently available
    pub fn available(&self) -> usize {
        self.available.load(Ordering::Relaxed)
    }

    /// Get the number of buffers currently in use
    pub fn in_use(&self) -> usize {
        self.capacity() - self.available()
    }
}

/// A buffer acquired from a [`BufferPool`].
///
/// Must be returned to the pool via [`BufferPool::release`] when done.
#[derive(Debug)]
pub struct PooledBuffer {
    /// The buffer data
    pub buffer: Vec<u8>,
    /// Index in the pool (for returning)
    index: usize,
}

impl PooledBuffer {
    /// Get the pool index of this buffer
    pub fn index(&self) -> usize {
        self.index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_memory_budget_defaults() {
        let budget = MemoryBudget::default();
        assert_eq!(budget.max_heap_bytes, 64 * 1024);
        assert_eq!(budget.max_message_size, 1024);
    }

    #[test]
    fn test_memory_tracker_allocation() {
        let tracker = MemoryTracker::new(MemoryBudget::minimal());

        // Should succeed
        let guard = tracker.try_allocate(1024).unwrap();
        assert_eq!(tracker.allocated_bytes(), 1024);

        // Should fail - exceeds budget
        let result = tracker.try_allocate(20_000);
        assert!(matches!(result, Err(MemoryError::BudgetExceeded { .. })));

        // Drop guard, memory freed
        drop(guard);
        assert_eq!(tracker.allocated_bytes(), 0);
    }

    #[test]
    fn test_connection_limit() {
        let budget = MemoryBudget {
            max_connections: 2,
            ..Default::default()
        };
        let tracker = MemoryTracker::new(budget);

        let _g1 = tracker.try_add_connection().unwrap();
        let _g2 = tracker.try_add_connection().unwrap();

        // Third connection should fail
        let result = tracker.try_add_connection();
        assert!(matches!(
            result,
            Err(MemoryError::ConnectionLimitReached { .. })
        ));
    }

    #[test]
    fn test_operation_queue_limit() {
        let budget = MemoryBudget {
            max_pending_ops: 2,
            ..Default::default()
        };
        let tracker = MemoryTracker::new(budget);

        let _g1 = tracker.try_queue_op().unwrap();
        let _g2 = tracker.try_queue_op().unwrap();

        // Third op should fail
        let result = tracker.try_queue_op();
        assert!(matches!(
            result,
            Err(MemoryError::OperationQueueFull { .. })
        ));
    }

    #[test]
    fn test_message_size_check() {
        let tracker = MemoryTracker::new(MemoryBudget::minimal());

        // Small message OK
        assert!(tracker.check_message_size(100).is_ok());

        // Large message rejected
        assert!(matches!(
            tracker.check_message_size(1000),
            Err(MemoryError::MessageTooLarge { .. })
        ));
    }

    #[test]
    fn test_buffer_pool_basic() {
        let mut pool = BufferPool::new(3, 256);

        assert_eq!(pool.capacity(), 3);
        assert_eq!(pool.available(), 3);
        assert_eq!(pool.in_use(), 0);
        assert_eq!(pool.buffer_size(), 256);

        // Acquire a buffer
        let buf1 = pool.try_acquire().unwrap();
        assert_eq!(buf1.buffer.len(), 256);
        assert_eq!(pool.available(), 2);
        assert_eq!(pool.in_use(), 1);

        // Acquire another
        let buf2 = pool.try_acquire().unwrap();
        assert_eq!(pool.available(), 1);

        // Release first buffer
        pool.release(buf1);
        assert_eq!(pool.available(), 2);

        // Release second buffer
        pool.release(buf2);
        assert_eq!(pool.available(), 3);
    }

    #[test]
    fn test_buffer_pool_exhaustion() {
        let mut pool = BufferPool::new(2, 64);

        let _b1 = pool.try_acquire().unwrap();
        let _b2 = pool.try_acquire().unwrap();

        // Pool exhausted
        assert!(pool.try_acquire().is_none());
    }

    #[test]
    #[should_panic(expected = "buffer_size must be positive")]
    fn test_buffer_pool_zero_size() {
        let _ = BufferPool::new(1, 0);
    }

    #[test]
    fn test_concurrent_allocation() {
        let tracker = Arc::new(MemoryTracker::new(MemoryBudget {
            max_heap_bytes: 10_000,
            ..Default::default()
        }));

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let tracker = Arc::clone(&tracker);
                thread::spawn(move || {
                    for _ in 0..100 {
                        if let Ok(guard) = tracker.try_allocate(100) {
                            // Hold briefly
                            thread::yield_now();
                            drop(guard);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All guards dropped, should be zero
        assert_eq!(tracker.allocated_bytes(), 0);
    }

    #[test]
    fn test_concurrent_connections() {
        let tracker = Arc::new(MemoryTracker::new(MemoryBudget {
            max_connections: 4,
            ..Default::default()
        }));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let tracker = Arc::clone(&tracker);
                thread::spawn(move || {
                    for _ in 0..50 {
                        if let Ok(guard) = tracker.try_add_connection() {
                            thread::yield_now();
                            drop(guard);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(tracker.connection_count(), 0);
    }

    #[test]
    fn test_concurrent_never_exceeds_limit() {
        use std::sync::atomic::AtomicBool;

        let tracker = Arc::new(MemoryTracker::new(MemoryBudget {
            max_heap_bytes: 1000,
            ..Default::default()
        }));
        let exceeded = Arc::new(AtomicBool::new(false));

        let handles: Vec<_> = (0..20)
            .map(|_| {
                let tracker = Arc::clone(&tracker);
                let exceeded = Arc::clone(&exceeded);
                thread::spawn(move || {
                    for _ in 0..100 {
                        // Check if we ever exceed the limit
                        let current = tracker.allocated_bytes();
                        if current > 1000 {
                            exceeded.store(true, Ordering::SeqCst);
                        }

                        if let Ok(guard) = tracker.try_allocate(100) {
                            let current = tracker.allocated_bytes();
                            if current > 1000 {
                                exceeded.store(true, Ordering::SeqCst);
                            }
                            drop(guard);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert!(!exceeded.load(Ordering::SeqCst), "Budget was exceeded!");
    }
}
