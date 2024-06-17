use std::time::Duration;
use libc::pid_t;
use crate::process::ExecuteAction::{Continue, Kill};

pub mod child;
/// The executor used for running sio2jail
pub mod executor;
pub mod data;
pub mod error;

#[derive(Debug)]
pub(crate) struct ExecuteEvent {
	pub(crate) pid: pid_t,
	pub(crate) exit_reason: ExitReason
}

#[derive(PartialEq, Eq, Debug)]
pub(crate) enum ExitReason {
	Exited { exit_status: i32 },
	Killed { signal: i32 },
	Stopped { signal: i32 },
	Trapped { signal: i32 }
}

#[derive(PartialEq, Eq, Debug)]
pub(crate) enum ExecuteAction {
	Continue,
	Kill
}

impl ExecuteAction {
	fn preserve_kill(&self, other: ExecuteAction) -> ExecuteAction {
		if *self == Kill || other == Kill {
			Kill
		} else {
			Continue
		}
	}
}

//TODO: THIS STRUCT IS A MESS
#[derive(Clone, Copy, Debug)]
pub struct ExecutionResult {
	pub instructions_used: Option<i64>,
	pub real_time: Option<Duration>,
	pub measured_time: Option<Duration>,
	pub exit_result: ExitResult
}

#[derive(Clone, Copy, Debug)]
pub enum ExitResult {
	Exited { exit_status: i32 },
	Killed { signal: i32, reason: KillReason }
}

impl ExecutionResult {
	pub(crate) fn new() -> ExecutionResult {
		ExecutionResult {
			instructions_used: None,
			real_time: None,
			measured_time: None,
			exit_result: ExitResult::Exited { exit_status: -1 },
		}
	}
}

#[derive(Clone, Copy, Debug)]
pub enum KillReason {
	NONE, RE, RV, TLE, MLE, OLE
}
