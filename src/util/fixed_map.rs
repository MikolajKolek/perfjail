use crate::util::signal_safe_spinlock::SignalSafeSpinlock;
use libc::{c_int, write};
use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

struct FixedMapBucketEntry {
    key: AtomicI32,
    value: AtomicI32,
    lock: SignalSafeSpinlock,
}

struct FixedMapBucket {
    values_vec: Vec<FixedMapBucketEntry>,
    values_ptr: *const FixedMapBucketEntry,
    length: AtomicUsize,
    bucket_modify_mutex: Mutex<()>,
}

impl FixedMapBucket {
    pub(crate) fn new(bucket_size: usize) -> Self {
        let mut values_vec = Vec::with_capacity(bucket_size);

        for _ in 0..bucket_size {
            values_vec.push(
                FixedMapBucketEntry {
                    key: AtomicI32::new(-1),
                    value: AtomicI32::new(0),
                    lock: SignalSafeSpinlock::new(),
                }
            )
        }

        let values_ptr = values_vec.as_ptr();
        FixedMapBucket {
            values_vec,
            values_ptr,
            length: AtomicUsize::new(0),
            bucket_modify_mutex: Mutex::new(()),
        }
    }
}

pub(crate) struct FixedMap {
    buckets_vec: Vec<FixedMapBucket>,
    buckets_ptr: *const FixedMapBucket,
    bucket_count: usize,
    bucket_size: usize,
    id_counter: AtomicU64,
}

impl FixedMap {
    fn hash(&self, key: c_int) -> usize {
        (key as usize) % self.bucket_count
    }

    fn modify_entry(&self, entry: &FixedMapBucketEntry, new_value: (i32, i32)) {
        entry.lock.with_lock(|| {
            entry.key.store(new_value.0, Ordering::Relaxed);
            entry.value.store(new_value.1, Ordering::Relaxed);
        });
    }

    pub(crate) fn new(bucket_count: usize, bucket_size: usize) -> Self {
        let mut buckets_vec = Vec::with_capacity(bucket_count);

        for _ in 0..bucket_count {
            buckets_vec.push(FixedMapBucket::new(bucket_size))
        }

        let buckets_ptr = buckets_vec.as_ptr();
        FixedMap {
            buckets_vec,
            buckets_ptr,
            bucket_count,
            bucket_size,
            id_counter: AtomicU64::new(0),
        }
    }

    pub(crate) fn insert(&mut self, key: c_int, value: c_int) {
        let hash = self.hash(key);
        let _guard = self.buckets_vec[hash].bucket_modify_mutex.lock().expect("Failed to acquire modify_lock");

        let length = self.buckets_vec[hash].length.load(Ordering::Acquire);
        let mut first_empty = length;
        for (i, entry) in self.buckets_vec[hash].values_vec.iter().enumerate().take(length) {
            let entry_key = entry.key.load(Ordering::Acquire);

            if entry_key == key {
                self.modify_entry(entry, (key, value));
                return;
            }
            if entry_key == -1 && first_empty == length {
                first_empty = i;
            }
        }

        if first_empty == length {
            if length == self.bucket_size {
                panic!("FixedMap bucket is full!");
            }

            self.modify_entry(&self.buckets_vec[hash].values_vec[first_empty], (key, value));
            self.buckets_vec[hash].length.store(length + 1, Ordering::Release);
        }
        else {
            self.modify_entry(&self.buckets_vec[hash].values_vec[first_empty], (key, value));
        }
    }

    pub(crate) fn remove(&self, key: c_int) {
        let hash = self.hash(key);
        let _guard = self.buckets_vec[hash].bucket_modify_mutex.lock().expect("Failed to acquire modify_lock");

        let length = self.buckets_vec[hash].length.load(Ordering::Acquire);
        for (i, entry) in self.buckets_vec[hash].values_vec.iter().enumerate().take(length) {
            if entry.key.load(Ordering::Acquire) == key {
                self.modify_entry(entry, (-1, 0));
                if i == length - 1 {
                    let mut to_decrease = 1;
                    while to_decrease < length &&
                        self.buckets_vec[hash].values_vec[i - to_decrease].key.load(Ordering::Acquire) == -1 {
                        to_decrease += 1;
                    }

                    self.buckets_vec[hash].length.store(length - to_decrease, Ordering::Release);
                }

                return;
            }
        }
    }

    pub(crate) fn get_and_write(&self, key: c_int) {
        let hash = self.hash(key);

        unsafe {
            let current = &*self.buckets_ptr.add(hash);
            let len = current.length.load(Ordering::Acquire);

            for i in 0..len {
                let entry = &*current.values_ptr.add(i);

                if entry.key.load(Ordering::Acquire) == key {
                    entry.lock.try_with_lock(|| {
                        if entry.key.load(Ordering::Relaxed) != key {
                            return;
                        }

                        let result = entry.value.load(Ordering::Relaxed);
                        let buf = [1u8];
                        write(result, buf.as_ptr() as *const c_void, buf.len());
                    });

                    return;
                }
            }
        }
    }
}