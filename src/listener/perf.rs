use std::ffi::{c_int, c_long, c_ulong, c_void};
use std::mem::{size_of_val, zeroed};
use std::time::Duration;

use libc::{__u64, close, pthread_barrier_destroy, pthread_barrier_init, pthread_barrier_t, pthread_barrier_wait, pthread_barrierattr_destroy, pthread_barrierattr_init, pthread_barrierattr_setpshared, pthread_barrierattr_t, PTHREAD_PROCESS_SHARED, read};
use perf_event_open_sys::bindings::{PERF_COUNT_HW_INSTRUCTIONS, perf_event_attr, PERF_FLAG_FD_CLOEXEC, PERF_FLAG_FD_NO_GROUP, PERF_TYPE_HARDWARE};
use perf_event_open_sys::perf_event_open;

use crate::listener::Listener;
use crate::process::data::{ExecutionData, ExecutionSettings};
use crate::process::ExecuteAction;
use crate::process::ExitResult::Killed;
use crate::process::KillReason::TLE;
use crate::util::{CYCLES_PER_SECOND, errno};

#[derive(Debug)]
pub(crate) struct PerfListener {
	barrier: pthread_barrier_t,
	perf_fd: Option<c_int>
}

impl PerfListener {
	pub(crate) fn new() -> PerfListener {
		let mut result = PerfListener {
			barrier: unsafe { zeroed() },
			perf_fd: None
		};
		
		unsafe {
			let barrier_: *mut pthread_barrier_t = &mut result.barrier as *mut pthread_barrier_t;

			let mut attr: pthread_barrierattr_t = zeroed();
			pthread_barrierattr_init(&mut attr);
			pthread_barrierattr_setpshared(&mut attr, PTHREAD_PROCESS_SHARED);
			pthread_barrier_init(barrier_, &attr, 2);
			pthread_barrierattr_destroy(&mut attr);
		}

		result
	}
}

impl Listener for PerfListener {
	fn on_post_fork_child(&mut self, _: &ExecutionSettings, _: &ExecutionData) {
		unsafe {
			pthread_barrier_wait(&mut self.barrier as *mut pthread_barrier_t);
		}
	}

	fn on_post_fork_parent(&mut self, _settings: &ExecutionSettings, data: &mut ExecutionData) {
		unsafe {
			let mut attrs: perf_event_attr = zeroed();
			attrs.type_ = PERF_TYPE_HARDWARE;
			attrs.size = size_of_val(&attrs) as u32;
			attrs.config = PERF_COUNT_HW_INSTRUCTIONS as __u64;
			attrs.set_exclude_user(0);
			attrs.set_exclude_kernel(1);
			attrs.set_exclude_hv(1);
			attrs.set_disabled(1);
			attrs.set_enable_on_exec(1);
			attrs.set_inherit(1);
			
			let perf_fd = perf_event_open(&mut attrs, data.pid.unwrap(), -1, -1, (PERF_FLAG_FD_NO_GROUP | PERF_FLAG_FD_CLOEXEC) as c_ulong);
			self.perf_fd = Some(perf_fd);

			pthread_barrier_wait(&mut self.barrier as *mut pthread_barrier_t);
			pthread_barrier_destroy(&mut self.barrier as *mut pthread_barrier_t);
		}
	}

	fn on_post_execute(&mut self, _: &ExecutionSettings, data: &mut ExecutionData) {
		data.execution_result.instructions_used = Some(self.get_instructions_used());
		data.execution_result.measured_time = Some(
			Duration::from_millis((data.execution_result.instructions_used.unwrap() * 1_000 / CYCLES_PER_SECOND) as u64)
		)
	}
	
	fn on_wakeup(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData) -> ExecuteAction {
		let instructions_used = self.get_instructions_used();

		if instructions_used > settings.instruction_count_limit.unwrap_or(i64::MAX) {
			data.execution_result.exit_result = Killed { signal: -1, reason: TLE };
			return ExecuteAction::Kill
		}

		ExecuteAction::Continue
	}
	
	fn get(&mut self) -> i64 {
		self.get_instructions_used()
	}
}

impl Drop for PerfListener {
	fn drop(&mut self) {
		if self.perf_fd.is_some() {
			unsafe { close(self.perf_fd.unwrap()); }
			self.perf_fd = None;
		}
	}
}

impl PerfListener {
	fn get_instructions_used(&mut self) -> i64 {
		let mut instructions_used: i64 = 0;

		unsafe {
			let size = read(self.perf_fd.unwrap(), &mut instructions_used as *mut c_long as *mut c_void, size_of_val(&instructions_used));

			if size != size_of_val(&instructions_used) as isize {
				panic!("ERROR {} {}\n\n", size, errno());
			}
			if instructions_used < 0 {
				panic!("ERROR2");
			}
		}

		instructions_used
	}
}