use std::cmp::min;
use std::ffi::{c_int, c_void, CString};
use std::mem::zeroed;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::path::PathBuf;
use std::ptr::null_mut;

use libc::{chdir, CLD_DUMPED, CLD_EXITED, CLD_KILLED, CLD_STOPPED, CLD_TRAPPED, close, dup2, execv, free, id_t, kill, P_PID, siginfo_t, SIGKILL, waitid, waitpid, WEXITED, WNOHANG, WNOWAIT, WSTOPPED};
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};

use crate::process::{ExecuteEvent, ExecutionResult, ExitResult};
use crate::process::data::ExecutionContext;
use crate::process::error::RunError;
use crate::process::ExecuteAction::{Continue, Kill};
use crate::process::ExitReason::{Exited, Killed, Stopped, Trapped};
use crate::process::KillReason::NONE;

pub struct Sio2jailChild {
	context: Box<ExecutionContext>,
	child_stack: *mut c_void
}

impl Sio2jailChild {
	pub(crate) fn new(context: Box<ExecutionContext>, child_stack: *mut c_void) -> Sio2jailChild {
		Sio2jailChild {
			context,
			child_stack
		}
	}

	pub fn run(&mut self) -> Result<ExecutionResult, RunError> {
		let poll_pid_fd = unsafe { PollFd::new(BorrowedFd::borrow_raw(self.context.data.pid_fd), PollFlags::POLLIN) };
		let mut poll_fds = [poll_pid_fd];
		
		self.context.listeners.iter_mut().for_each(|listener| listener.on_post_fork_parent(&self.context.settings, &mut self.context.data));

		loop {
			let mut timeout: Option<i32> = None;
			let mut action = Continue;
			self.context.listeners.iter_mut().for_each(|listener| {
				let (listener_action, listener_timeout) = listener.on_wakeup(&self.context.settings, &mut self.context.data);
				action = action.preserve_kill(listener_action);
				
				if let Some(mut timeout) = timeout {
					timeout = min(timeout, listener_timeout.unwrap_or(i32::MAX));
				} else {
					timeout = listener_timeout;
				}
			});
			
			if action == Kill {
				self.kill_child();
			}

			let poll_result = poll(&mut poll_fds, PollTimeout::try_from(timeout.unwrap_or(-1)).unwrap()).unwrap();
			println!("{}", poll_result);
			if poll_result == 0 {
				// This means that one of the listeners' timeouts has finished, and we need to call all the on_wakeup functions again
				continue;
			}
			
			let mut wait_info: siginfo_t = unsafe { zeroed() };
			let return_value: c_int;
			unsafe {
				return_value = waitid(P_PID, self.context.data.pid.unwrap() as id_t, &mut wait_info as *mut siginfo_t, WEXITED | WSTOPPED | WNOWAIT);
			}

			if return_value == -1 {
				panic!("oopsie")
			}

			let event: ExecuteEvent;
			unsafe {
				event = ExecuteEvent {
					pid: wait_info.si_pid(),
					exit_reason: match wait_info.si_code {
						code if code == CLD_EXITED => {
							Exited { exit_status: wait_info.si_status() }
						},
						code if code == CLD_KILLED || code == CLD_DUMPED => {
							Killed { signal: wait_info.si_status() }
						},
						code if code == CLD_STOPPED => {
							Stopped { signal: wait_info.si_status() }
						},
						code if code == CLD_TRAPPED => {
							Trapped { signal: wait_info.si_status() }
						},
						_ => { panic!("duck") }
					}
				}
			}

			if matches!(event.exit_reason, Exited { .. }) || matches!(event.exit_reason, Killed { .. }) {
				if event.pid == self.context.data.pid.unwrap() {
					if let Exited { exit_status: status} = event.exit_reason {
						self.context.data.execution_result.exit_result = ExitResult::Exited { exit_status: status }
					} else if let Killed { signal: kill_signal } = event.exit_reason {
						self.context.data.execution_result.exit_result = ExitResult::Killed { signal: kill_signal, reason: NONE }
					}
					
					break;
				} else {
					unsafe {
						waitid(P_PID, event.pid as id_t, null_mut::<siginfo_t>(), WEXITED | WSTOPPED | WNOHANG);
					}
				}
			}
		}

		self.context.listeners.iter_mut().for_each(|listener| listener.on_post_execute(&self.context.settings, &mut self.context.data));
		unsafe {
			waitpid(-1, null_mut::<c_int>(), WNOHANG);
			close(self.context.data.pid_fd);
		}

		Ok(self.context.data.execution_result)
	}

	fn kill_child(&mut self) {
		if self.context.data.pid.is_some() {
			unsafe {
				kill(self.context.data.pid.unwrap(), SIGKILL);
				free(self.child_stack);
			}
			
			self.context.data.execution_result.exit_result = ExitResult::Killed { signal: SIGKILL, reason: NONE };
			self.context.data.pid = None;
		}
	}
}

impl Drop for Sio2jailChild {
	fn drop(&mut self) {
		self.kill_child();
	}
}

pub(crate) extern "C" fn execute_child(memory: *mut c_void) -> c_int {
	let context_ptr = memory as *mut ExecutionContext;
	let context = unsafe { &mut (*context_ptr) };

	context.listeners.iter_mut().for_each(|listener| listener.on_post_fork_child(&context.settings, &context.data));

	if context.settings.working_dir != PathBuf::new() {
		unsafe {
			let path_c_str = CString::new(context.settings.working_dir.to_str().expect("Couldn't convert working_dir to str").as_bytes()).expect("Couldn't convert working_dir to CString");
			chdir(path_c_str.as_ptr());
		}
	}

	unsafe {
		if let Some(stdin_fd) = context.settings.stdin_fd.as_ref() {
			dup2(stdin_fd.as_raw_fd(), 0 as c_int);
			close(stdin_fd.as_raw_fd());
		}

		if let Some(stdout_fd) = context.settings.stdout_fd.as_ref() {
			dup2(stdout_fd.as_raw_fd(), 1 as c_int);
			close(stdout_fd.as_raw_fd());
		}

		if let Some(stderr_fd) = context.settings.stderr_fd.as_ref() {
			dup2(stderr_fd.as_raw_fd(), 2 as c_int);
			close(stderr_fd.as_raw_fd());
		}
	}
	
	unsafe {
		execv(context.settings.executable_path.as_ptr(), context.settings.args_ptr.as_ptr());
	}

	panic!("AAAA add error handling here")
}