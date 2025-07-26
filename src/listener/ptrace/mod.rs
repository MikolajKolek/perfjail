mod process_info;
mod tracee;

use crate::listener::ptrace::process_info::ProcessInfo;
use crate::listener::{Listener, WakeupAction};
use crate::process::data::{ExecutionData, ExecutionSettings};
use nix::sys::ptrace::{attach, cont, setoptions, Options};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::Pid;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::LazyLock;

static PTRACE_OPTIONS: LazyLock<Options> = LazyLock::new(|| {
    Options::PTRACE_O_EXITKILL |
    Options::PTRACE_O_TRACESECCOMP |
    Options::PTRACE_O_TRACEEXEC |
    Options::PTRACE_O_TRACECLONE
});

#[derive(Debug)]
pub(crate) struct PtraceListener {
    has_execd: bool
}

impl PtraceListener {
    pub(crate) fn new() -> PtraceListener {
        PtraceListener {
            has_execd: false
        }
    }
}

impl Listener for PtraceListener {
    fn requires_timeout(&self, _: &ExecutionSettings) -> bool {
        false
    }

    fn on_post_clone_child(&self, _: &ExecutionSettings, _: &ExecutionData) -> std::io::Result<()> {
        Ok(())
    }

    fn on_post_clone_parent(&mut self, _: &ExecutionSettings, data: &mut ExecutionData) -> std::io::Result<()> {
        let root_pid = data.pid.expect("child pid not set");
        self.root_process_info = Some(ProcessInfo::new(root_pid));

        attach(Pid::from_raw(root_pid))?;
        waitpid(Pid::from_raw(root_pid), None)?;
        setoptions(Pid::from_raw(root_pid), *PTRACE_OPTIONS)?;
        cont(Pid::from_raw(root_pid), None)?;

        Ok(())
    }

    fn on_wakeup(&mut self, _: &ExecutionSettings, _: &mut ExecutionData) -> std::io::Result<WakeupAction> {
        Ok(WakeupAction::Continue)
    }

    fn on_execute_event(
        &mut self,
        settings: &ExecutionSettings,
        data: &mut ExecutionData,
        status: &WaitStatus
    ) -> io::Result<WakeupAction> {
        if let WaitStatus::PtraceEvent(pid, signal, event) = status {

        }

        Ok(WakeupAction::Continue)
    }

    fn on_post_execute(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData) -> std::io::Result<()> {
        //todo!()
        Ok(())
    }
}

impl PtraceListener {

}