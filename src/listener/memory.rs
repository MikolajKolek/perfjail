use crate::listener::WakeupAction::{Continue, Kill};
use crate::listener::{Listener, WakeupAction};
use crate::process::data::{ExecutionContext, ExecutionSettings, ParentData};
use crate::process::ExitStatus;
use libc::c_int;
use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::sys::resource::{getrlimit, setrlimit, Resource};
use nix::sys::wait::WaitStatus;
use nix::unistd::{close, pipe2, read};
use std::cell::UnsafeCell;
use std::os::fd::{BorrowedFd, IntoRawFd, RawFd};
use std::{fs, io};

#[derive(Debug)]
pub(crate) struct MemoryListener {
    child: RawFd,
    parent: RawFd,
    closed_child_in_parent: UnsafeCell<bool>,
    peak_memory_kibibytes: UnsafeCell<u64>,
}

impl MemoryListener {
    pub(crate) fn new() -> Self {
        let (read, write) = pipe2(OFlag::O_CLOEXEC | OFlag::O_NONBLOCK).expect(
            "Failed to create pipe for memory limit listener",
        );

        MemoryListener {
            child: write.into_raw_fd(),
            parent: read.into_raw_fd(),
            closed_child_in_parent: UnsafeCell::new(false),
            peak_memory_kibibytes: UnsafeCell::new(0),
        }
    }
}

impl Listener for MemoryListener {
    fn requires_timeout(&self, settings: &ExecutionSettings) -> bool {
        settings.memory_limit_kibibytes.is_some()
    }

    fn on_post_clone_child(&self, _: &ExecutionContext) -> nix::Result<()> {
        close(self.parent)?;

        // Set address space and stack limits to the highest possible value (usually infinity)
        let (_, hard_as_limit) = getrlimit(Resource::RLIMIT_AS)?;
        setrlimit(Resource::RLIMIT_AS, hard_as_limit, hard_as_limit)?;
        let (_, hard_stack_limit) = getrlimit(Resource::RLIMIT_STACK)?;
        setrlimit(Resource::RLIMIT_STACK, hard_stack_limit, hard_stack_limit)?;

        Ok(())
    }

    fn on_post_clone_parent(&self, _: &ExecutionContext, _: &mut ParentData) -> io::Result<()> {
        close(self.child)?;

        unsafe {
            *self.closed_child_in_parent.get() = true;
        }

        Ok(())
    }

    fn on_wakeup(&self, context: &ExecutionContext, parent_data: &mut ParentData) -> io::Result<WakeupAction> {
        if self.was_exec_called() {
            unsafe {
                *self.peak_memory_kibibytes.get() = (*self.peak_memory_kibibytes.get()).max(
                    MemoryListener::get_peak_memory_usage(parent_data.pid).unwrap_or(0)
                );
            }

            if let Some(limit) = context.settings.memory_limit_kibibytes && unsafe { *self.peak_memory_kibibytes.get() } > limit {
                parent_data.execution_result.set_exit_status(ExitStatus::MLE("memory limit exceeded".into()));
                return Ok(Kill)
            }
        }

        Ok(Continue)
    }

    fn on_execute_event(&self, _: &ExecutionContext, _: &mut ParentData, _: &WaitStatus) -> io::Result<WakeupAction> {
        Ok(Continue)
    }

    fn on_post_execute(&self, context: &ExecutionContext, parent_data: &mut ParentData) -> io::Result<()> {
        parent_data.execution_result.set_memory_usage_kibibytes(unsafe { *self.peak_memory_kibibytes.get() });
        if let Some(limit) = context.settings.memory_limit_kibibytes && unsafe { *self.peak_memory_kibibytes.get() } > limit {
            parent_data.execution_result.set_exit_status(ExitStatus::MLE("memory limit exceeded".into()));
        }

        Ok(())
    }
}

impl Drop for MemoryListener {
    // We only concern ourselves with drop for the parent, as the
    // listener won't be dropped in the child
    fn drop(&mut self) {
        if !unsafe { *self.closed_child_in_parent.get() } {
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

    fn get_peak_memory_usage(pid: c_int) -> Option<u64> {
        let status =
            fs::read_to_string(format!("/proc/{}/status", pid))
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