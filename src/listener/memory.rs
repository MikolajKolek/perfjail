use crate::listener::WakeupAction::{Continue, Kill};
use crate::listener::{Listener, WakeupAction};
use crate::process::data::{ExecutionData, ExecutionSettings};
use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::unistd::{close, pipe2, read};
use std::os::fd::{BorrowedFd, IntoRawFd, RawFd};
use std::{fs, io};
use nix::sys::resource::{getrlimit, setrlimit, Resource};
use nix::sys::wait::WaitStatus;
use crate::process::ExitStatus;

#[derive(Debug)]
pub(crate) struct MemoryListener {
    child: RawFd,
    parent: RawFd,
    closed_child_in_parent: bool,
    peak_memory_kibibytes: u64,
}

impl MemoryListener {
    pub(crate) fn new() -> Self {
        let (read, write) = pipe2(OFlag::O_CLOEXEC | OFlag::O_NONBLOCK).expect(
            "Failed to create pipe for memory limit listener",
        );

        MemoryListener {
            child: write.into_raw_fd(),
            parent: read.into_raw_fd(),
            closed_child_in_parent: false,
            peak_memory_kibibytes: 0,
        }
    }
}

impl Listener for MemoryListener {
    fn requires_timeout(&self, settings: &ExecutionSettings) -> bool {
        settings.memory_limit_kibibytes.is_some()
    }

    fn on_post_clone_child(&self, _: &ExecutionSettings, _: &ExecutionData) -> io::Result<()> {
        close(self.parent)?;

        // Set address space and stack limits to the highest possible value (usually infinity)
        let (_, hard_as_limit) = getrlimit(Resource::RLIMIT_AS)?;
        setrlimit(Resource::RLIMIT_AS, hard_as_limit, hard_as_limit)?;
        let (_, hard_stack_limit) = getrlimit(Resource::RLIMIT_STACK)?;
        setrlimit(Resource::RLIMIT_STACK, hard_stack_limit, hard_stack_limit)?;

        Ok(())
    }

    fn on_post_clone_parent(&mut self, _: &ExecutionSettings, _: &mut ExecutionData) -> io::Result<()> {
        close(self.child)?;
        self.closed_child_in_parent = true;
        Ok(())
    }

    fn on_wakeup(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData) -> io::Result<WakeupAction> {
        if self.was_exec_called() {
            self.peak_memory_kibibytes = self.peak_memory_kibibytes.max(
                MemoryListener::get_peak_memory_usage(data).unwrap_or(0)
            );

            if let Some(limit) = settings.memory_limit_kibibytes && self.peak_memory_kibibytes > limit {
                data.execution_result.set_exit_status(ExitStatus::MLE("memory limit exceeded".into()));
                return Ok(Kill)
            }
        }

        Ok(Continue)
    }

    fn on_execute_event(&mut self, _: &ExecutionSettings, _: &mut ExecutionData, _: &WaitStatus) -> io::Result<WakeupAction> {
        Ok(Continue)
    }

    fn on_post_execute(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData) -> io::Result<()> {
        data.execution_result.set_memory_usage_kibibytes(self.peak_memory_kibibytes);
        if let Some(limit) = settings.memory_limit_kibibytes && self.peak_memory_kibibytes > limit {
            data.execution_result.set_exit_status(ExitStatus::MLE("memory limit exceeded".into()));
        }

        Ok(())
    }
}

impl Drop for MemoryListener {
    // We only concern ourselves with drop for the parent, as the
    // listener won't be dropped in the child
    fn drop(&mut self) {
        if !self.closed_child_in_parent {
            close(self.child).expect("Failed to close child pipe");
        }

        close(self.parent).expect("Failed to close parent pipe");
    }
}

impl MemoryListener {
    fn was_exec_called(&self) -> bool {
        let mut buf = [0u8; 1];

        loop {
            match read(unsafe { BorrowedFd::borrow_raw(self.parent) }, &mut buf) {
                Ok(0) => return true,
                Err(Errno::EAGAIN) => return false,
                Err(Errno::EINTR) => continue,
                _ => panic!("unexpected result from pipe read")
            }
        }
    }

    fn get_peak_memory_usage(data: &ExecutionData) -> Option<u64> {
        let status =
            fs::read_to_string(format!("/proc/{}/status", data.pid.expect("pid not set")))
            .expect("Failed to read /proc/<pid>/status");

        if let Some(peak) =
            status
            .split("\n")
            .find(|line| line.starts_with("VmPeak:"))
        {
            Some(
                peak.split_whitespace()
                    .nth(1)
                    .expect("VmPeak value not found")
                    .parse::<u64>()
                    .expect("VmPeak value is not a number")
            )
        } else {
            None
        }
    }
}