use std::hint::spin_loop;
use std::sync::atomic::{AtomicBool, Ordering};

/// A spinlock allowing for both waiting until the lock can be acquired, or trying to acquire the lock and
/// returning immediately if it fails.
///
/// The second functionality allows for its safe use inside signal handlers.
pub(crate) struct SignalSafeSpinlock {
    state: AtomicBool,
}

impl SignalSafeSpinlock {
    pub const fn new() -> Self {
        Self {
            state: AtomicBool::new(false),
        }
    }

    /// Runs the provided function with the lock, passing on it's return value.
    ///
    /// If the lock can't be acquired immediately, loops until it can be acquired.
    pub fn with_lock<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        while self
            .state
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spin_loop();
        }

        let _guard = SpinLockGuard { lock: &self.state };
        f()
    }

    /// Runs the provided function with the lock, passing on it's return value.
    ///
    /// If the lock can't be acquired immediately, returns [None]. This makes this function async-signal-safe to run,
    /// as long as [f] is async-signal-safe.
    pub fn try_with_lock<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce() -> R,
    {
        if self
            .state
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            let _guard = SpinLockGuard { lock: &self.state };
            Some(f())
        } else {
            None
        }
    }
}

pub struct SpinLockGuard<'a> {
    lock: &'a AtomicBool,
}

impl<'a> Drop for SpinLockGuard<'a> {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
    }
}