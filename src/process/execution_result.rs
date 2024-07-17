use std::time::Duration;

use crate::util::CYCLES_PER_SECOND;

#[readonly::make]
#[derive(Clone, Debug)]
pub struct ExecutionResult {
	pub exit_status: ExitStatus,
	pub exit_reason: ExitReason,
	/// Testing comment for instructions used
	pub instructions_used: Option<i64>,
	pub measured_time: Option<Duration>,
	pub real_time: Option<Duration>,
	pub user_time: Option<Duration>,
	pub system_time: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExitStatus {
	OK,
	RE(String),
	RV(String),
	TLE(String),
	MLE(String),
	OLE(String),
}

#[derive(Clone, Copy, Debug)]
pub enum ExitReason {
	Exited { exit_status: i32 },
	Killed { signal: i32 },
}

impl ExitStatus {
	pub fn get_exit_status_comment(&self) -> String {
		match self {
			ExitStatus::OK => String::new(),
			ExitStatus::RE(comment) => comment.clone(),
			ExitStatus::RV(comment) => comment.clone(),
			ExitStatus::TLE(comment) => comment.clone(),
			ExitStatus::MLE(comment) => comment.clone(),
			ExitStatus::OLE(comment) => comment.clone(),
		}
	}
}

impl ExecutionResult {
	pub(crate) fn new() -> ExecutionResult {
		ExecutionResult {
			exit_status: ExitStatus::OK,
			exit_reason: ExitReason::Exited { exit_status: 0 },
			instructions_used: None,
			measured_time: None,
			real_time: None,
			user_time: None,
			system_time: None,
		}
	}

	pub(crate) fn set_exit_status(&mut self, exit_status: ExitStatus) {
		if self.exit_status == ExitStatus::OK {
			self.exit_status = exit_status;
		}
	}

	pub(crate) fn set_exit_result(&mut self, exit_reason: ExitReason) {
		self.exit_reason = exit_reason
	}

	pub(crate) fn set_instructions_used(&mut self, instructions_used: i64) {
		self.instructions_used = Some(instructions_used);
		self.measured_time = Some(Duration::from_millis(
			(instructions_used * 1_000 / CYCLES_PER_SECOND) as u64,
		))
	}

	pub(crate) fn set_real_time(&mut self, real_time: Duration) {
		self.real_time = Some(real_time)
	}

	pub(crate) fn set_user_time(&mut self, user_time: Duration) {
		self.user_time = Some(user_time)
	}

	pub(crate) fn set_system_time(&mut self, system_time: Duration) {
		self.system_time = Some(system_time)
	}
}
