use crate::listener::Listener;
use crate::process::error::RunError;
use crate::process::execution_result::ExecutionResult;
use crate::process::jail::Perfjail;
use crate::util::CHILD_STACK_SIZE;
use std::ffi::{c_int, CString};
use std::os::fd::{BorrowedFd, OwnedFd};
use std::path::PathBuf;
use std::sync::Barrier;
use std::time::Duration;

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
    pub(crate) raw_pid_fd: c_int,
    pub(crate) pid: Option<c_int>,
    pub(crate) execution_result: ExecutionResult,
    pub(crate) child_error: Option<RunError>,
    pub(crate) child_stack: [u8; CHILD_STACK_SIZE],
    pub(crate) clone_barrier: Barrier
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
            raw_pid_fd: 0,
            pid: None,
            execution_result: ExecutionResult::new(),
            child_error: None,
            child_stack: unsafe { std::mem::zeroed() },
            clone_barrier: Barrier::new(2)
        }
    }
}
