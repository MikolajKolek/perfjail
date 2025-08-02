use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::AtomicU8;
use core::sync::atomic::Ordering;

const UNINITIALIZED: u8 = 0;
const INITIALIZING: u8 = 1;
const INITIALIZED: u8 = 2;

#[derive(Debug)]
pub(crate) struct AtomicOnceLock<T> {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Sync> Sync for AtomicOnceLock<T> {}
unsafe impl<T: Send> Send for AtomicOnceLock<T> {}

impl<T> AtomicOnceLock<T> {
    pub(crate) const fn new() -> Self {
        Self {
            state: AtomicU8::new(UNINITIALIZED),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Set the value if not already set.
    pub(crate) fn set(&self, val: T) {
        if self
            .state
            .compare_exchange(UNINITIALIZED, INITIALIZING, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            unsafe {
                (&mut *self.value.get()).write(val);
            }

            self.state.store(INITIALIZED, Ordering::Release);
        }
    }

    /// Get a reference to the value if initialized
    pub(crate) fn get(&self) -> Option<&T> {
        if self.state.load(Ordering::Acquire) == INITIALIZED {
            Some(unsafe { (*self.value.get()).assume_init_ref() })
        } else {
            None
        }
    }
}
