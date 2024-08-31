use std::ffi::{c_int, CString};
use std::os::fd::{BorrowedFd, OwnedFd};
use std::path::PathBuf;
use std::time::Duration;

use crate::listener::Listener;
use crate::process::error::RunError;
use crate::process::execution_result::ExecutionResult;
use crate::process::jail::Perfjail;

#[derive(Debug)]
pub(crate) struct ExecutionContext<'a> {
    pub(crate) settings: ExecutionSettings<'a>,
    pub(crate) data: ExecutionData,
    pub(crate) listeners: Vec<Box<dyn Listener>>,
}

#[readonly::make]
#[derive(Debug)]
pub(crate) struct ExecutionSettings<'a> {
    pub(crate) real_time_limit: Option<Duration>,
    pub(crate) instruction_count_limit: Option<i64>,
    pub(crate) executable_path: CString,
    pub(crate) args: Vec<CString>,
    pub(crate) working_dir: PathBuf,
    pub(crate) stdin_fd: Option<BorrowedFd<'a>>,
    pub(crate) stdout_fd: Option<BorrowedFd<'a>>,
    pub(crate) stderr_fd: Option<BorrowedFd<'a>>,
}

#[derive(Debug)]
pub(crate) struct ExecutionData {
    pub(crate) pid_fd: Option<OwnedFd>,
    pub(crate) pid: Option<c_int>,
    pub(crate) execution_result: ExecutionResult,
    pub(crate) child_error: Option<RunError>,
}

impl ExecutionSettings<'_> {
    pub(crate) fn new(executor: Perfjail) -> ExecutionSettings {
        ExecutionSettings {
            real_time_limit: executor.real_time_limit,
            instruction_count_limit: executor.instruction_count_limit,
            executable_path: executor.executable_path,
            args: executor.args,
            working_dir: executor.working_dir,
            stdin_fd: executor.stdin_fd,
            stdout_fd: executor.stdout_fd,
            stderr_fd: executor.stderr_fd,
        }
    }
}

impl ExecutionData {
    pub(crate) fn new() -> ExecutionData {
        ExecutionData {
            pid_fd: None,
            pid: None,
            execution_result: ExecutionResult::new(),
            child_error: None,
        }
    }
}
