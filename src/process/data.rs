use crate::listener::Listener;
use crate::process::execution_result::ExecutionResult;
use crate::process::jail::Perfjail;
use std::ffi::{c_int, CString};
use std::os::fd::BorrowedFd;
use std::os::raw::c_void;
use std::path::PathBuf;
use std::time::Duration;
use sync_linux_no_libc::sync::Barrier;
use crate::util::atomic_once_lock::AtomicOnceLock;

#[derive(Debug)]
pub(crate) struct ExecutionContext<'a> {
    pub(crate) settings: ExecutionSettings<'a>,
    pub(crate) data: SharedData,
    pub(crate) listeners: Vec<Box<dyn Listener>>,
}

#[readonly::make]
#[derive(Debug)]
pub(crate) struct ExecutionSettings<'a> {
    pub(crate) real_time_limit: Option<Duration>,
    pub(crate) user_time_limit: Option<Duration>,
    pub(crate) system_time_limit: Option<Duration>,
    pub(crate) user_system_time_limit: Option<Duration>,
    pub(crate) instruction_count_limit: Option<i64>,
    pub(crate) memory_limit_kibibytes: Option<u64>,
    pub(crate) executable_path: CString,
    pub(crate) args: Vec<CString>,
    pub(crate) working_dir: Option<PathBuf>,
    pub(crate) stdin_fd: Option<BorrowedFd<'a>>,
    pub(crate) stdout_fd: Option<BorrowedFd<'a>>,
    pub(crate) stderr_fd: Option<BorrowedFd<'a>>,
}

#[derive(Debug)]
pub(crate) struct SharedData {
    pub(crate) child_error: AtomicOnceLock<nix::Error>,
    pub(crate) child_ready_barrier: Barrier,
    pub(crate) parent_ready_barrier: Barrier,
}

#[derive(Debug)]
pub(crate) struct ParentData {
    pub(crate) child_stack: Box<c_void>,
    pub(crate) pid: c_int,
    pub(crate) execution_result: ExecutionResult,
}

impl ExecutionSettings<'_> {
    pub(crate) fn new(executor: Perfjail) -> ExecutionSettings {
        ExecutionSettings {
            real_time_limit: executor.real_time_limit,
            user_time_limit: executor.user_time_limit,
            system_time_limit: executor.system_time_limit,
            user_system_time_limit: executor.user_system_time_limit,
            instruction_count_limit: executor.instruction_count_limit,
            memory_limit_kibibytes: executor.memory_limit_kibibytes,
            executable_path: executor.executable_path,
            args: executor.args,
            working_dir: executor.working_dir,
            stdin_fd: executor.stdin_fd,
            stdout_fd: executor.stdout_fd,
            stderr_fd: executor.stderr_fd,
        }
    }
}

impl SharedData {
    pub(crate) fn new() -> SharedData {
        SharedData {
            child_error: AtomicOnceLock::new(),
            child_ready_barrier: Barrier::new(2),
            parent_ready_barrier: Barrier::new(2),
        }
    }
}
