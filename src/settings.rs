use crate::listener::perf::sighandler::init_sighandler;

/// Sets the maximum number of [crate::process::Perfjail] instances that may be running at the same time
/// with [crate::process::Perfjail::measured_time_limit] or [crate::process::Perfjail::instruction_count_limit]
/// set.
///
/// If this function is not called at all before running Perfjail instances with these options, they will crash.
///
/// This value cannot be surpassed, or Perfjail might randomly crash. The value also can't be modified mid-execution.
///
/// # Panics
/// Panics if this function has been called more than once.
pub fn set_perf_timeout_thread_count(count: usize) {
    if !init_sighandler(count) {
        panic!("perfjail::set_perf_timeout_thread_count called more than once");
    }
}