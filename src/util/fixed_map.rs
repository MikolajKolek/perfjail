use libc::c_int;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, Ordering};
use std::sync::Mutex;
use crate::util::sig_atomic_t;

struct FixedMapBucketEntry<V> {
    key: sig_atomic_t,
    value: *const V,
    modifying: sig_atomic_t,
    id: i32,
}

struct FixedMapBucket<V> {
    values: *mut FixedMapBucketEntry<V>,
    length: sig_atomic_t,
    modify_lock: AtomicBool,
}

//TODO: Needs a full rework with LOCK CMPXCHG
/// A hashmap maintaining some very specific properties:
/// - It maintains a fixed capacity provided at the time of creation
/// - It guarantees that the [FixedMap::get] operation is async-signal-safe
/// - For a given key, it maintains a set order of operations:
///     - (Thread 1) Insert, {(Any thread) Read, (Any thread) Read, ...}, (Thread 1) Delete
///     - (Thread 2) Insert, {(Any thread) Read, (Any thread) Read, ...}, (Thread 2) Delete
///     - ...
pub(crate) struct FixedMap<V> {
    buckets: *mut FixedMapBucket<V>,
    length: sig_atomic_t,
}

impl<V> FixedMap<V> {
    pub(crate) fn new(capacity: usize) -> Self {
        let mut vec = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            vec.push(
                FixedMapEntry {
                    start: None,
                    insert_lock: AtomicBool::new(false),
                    delete_lock: AtomicBool::new(false),
                }
            )
        }

        let test: AtomicU8;
        test.compare_exchange()

        FixedMap {
            vec,
        }
    }

    fn hash(&self, key: c_int) -> usize {
        (key % self.length) as usize
    }

    pub(crate) fn insert(&mut self, key: c_int, value: *mut V) {
        let hash = self.hash(key);

        while self.vec[hash].insert_lock.swap(true, Ordering::Acquire) {
            std::hint::spin_loop();
        }

        self.vec[hash].start = Some(
            Box::new(
                FixedMapListEntry {
                    key,
                    value: AtomicPtr::new(value),
                    next: self.vec[hash].start.take()
                }
            )
        );

        self.vec[hash].insert_lock.store(false, Ordering::Release);
    }

    pub(crate) fn remove(&self, key: c_int) {

    }

    /// Get is guaranteed to be async-signal-safe
    pub(crate) fn get(&self, key: sig_atomic_t) -> Option<*const V> {
        let hash = self.hash(key);

        unsafe {
            let current = &*self.buckets.add(hash);
            let len = current.length;

            for i in 0..len {
                let entry = &*current.values.add(i as usize);

                if entry.key == key {
                    return Some(entry.value)
                }
            }
        }

        Mutex::new(None);
        let test: AtomicPtr<>;
        None
    }
}