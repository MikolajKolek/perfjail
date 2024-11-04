use std::time::Duration;

use crate::util::CYCLES_PER_SECOND;

#[readonly::make]
#[derive(Clone, Debug)]
pub struct ExecutionResult {
    pub exit_status: ExitStatus,
    pub exit_reason: ExitReason,
    /// The number of CPU instructions executed by the child program.
    ///
    /// This value is returned only if the [`PERF`](crate::process::Feature::PERF) feature flag is enabled.
    pub instructions_used: Option<i64>,
    /// The amount of time measured using the sio2jail method (1 second = 2_000_000_000 CPU instructions) during the execution of the child program.
    ///
    /// This value is returned only if the [`PERF`](crate::process::Feature::PERF) feature flag is enabled.
    pub measured_time: Option<Duration>,
    /// The amount of real time passed during the execution of the child program.
    pub real_time: Duration,
    /// The amount of user time passed during the execution of the child program.
    pub user_time: Duration,
    /// The amount of system time passed during the execution of the child program.
    pub system_time: Duration,
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
            real_time: Duration::ZERO,
            user_time: Duration::ZERO,
            system_time: Duration::ZERO,
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
        self.real_time = real_time
    }

    pub(crate) fn set_user_time(&mut self, user_time: Duration) {
        self.user_time = user_time
    }

    pub(crate) fn set_system_time(&mut self, system_time: Duration) {
        self.system_time = system_time
    }
}
