use std::ffi::{c_int, CString};
use std::io;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::ptr::null;
use std::time::Duration;

use libc::c_char;

use crate::listener::Listener;
use crate::process::ExecutionResult;
use crate::process::executor::Sio2jailExecutor;

#[derive(Debug)]
pub(crate) struct ExecutionContext {
	pub(crate) settings: ExecutionSettings,
	pub(crate) data: ExecutionData,
	pub(crate) listeners: Vec<Box<dyn Listener>>
}

#[allow(dead_code)]
#[readonly::make]
#[derive(Debug)]
pub(crate) struct ExecutionSettings {
	pub(crate) real_time_limit: Option<Duration>,
	pub(crate) instruction_count_limit: Option<i64>,
	pub(crate) executable_path: CString,
	pub(crate) args: Vec<CString>,
	pub(crate) args_ptr: Vec<*const c_char>,
	pub(crate) working_dir: PathBuf,
	pub(crate) stdin_fd: Option<OwnedFd>,
	pub(crate) stdout_fd: Option<OwnedFd>,
	pub(crate) stderr_fd: Option<OwnedFd>,
	pub(crate) oversampling_factor: i32
}

#[derive(Debug)]
pub(crate) struct ExecutionData {
	pub(crate) pid_fd: c_int,
	pub(crate) pid: Option<c_int>,
	pub(crate) execution_result: ExecutionResult,
	pub(crate) child_error: io::Result<()>
}

impl ExecutionSettings {
	pub(crate) fn new(executor: Sio2jailExecutor) -> ExecutionSettings {
		let mut result = ExecutionSettings {
			real_time_limit: executor.real_time_limit,
			instruction_count_limit: executor.instruction_count_limit,
			executable_path: executor.executable_path,
			args: executor.args,
			args_ptr: vec![],
			working_dir: executor.working_dir,
			stdin_fd: executor.stdin_fd,
			stdout_fd: executor.stdout_fd,
			stderr_fd: executor.stderr_fd,
			oversampling_factor: executor.oversampling_factor.unwrap_or(2)
		};
		
		for arg in &result.args {
			result.args_ptr.push(arg.as_ptr());
		}
		result.args_ptr.push(null());
		
		result
	}
}

impl ExecutionData {
	pub(crate) fn new() -> ExecutionData {
		ExecutionData {
			pid_fd: -1,
			pid: None,
			execution_result: ExecutionResult::new(),
			child_error: Ok(()),
		}
	}
}