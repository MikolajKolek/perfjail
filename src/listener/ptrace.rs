use crate::listener::{Listener, WakeupAction};
use crate::process::data::{ExecutionData, ExecutionSettings};
use nix::sys::ptrace::{attach, cont, setoptions, Options};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::Pid;
use std::io;
use std::sync::LazyLock;
use nix::sys::signal::kill;

static PTRACE_OPTIONS: LazyLock<Options> = LazyLock::new(|| {
    Options::PTRACE_O_EXITKILL |
    Options::PTRACE_O_TRACEEXIT
});

#[derive(Debug)]
pub(crate) struct PtraceListener {}

impl PtraceListener {
    pub(crate) fn new() -> PtraceListener {
        PtraceListener {}
    }
}

impl Listener for PtraceListener {
    fn requires_timeout(&self, _: &ExecutionSettings) -> bool {
        false
    }

    fn on_post_clone_child(&self, _: &ExecutionSettings, _: &ExecutionData) -> io::Result<()> {
        Ok(())
    }

    fn on_post_clone_parent(&mut self, _: &ExecutionSettings, data: &mut ExecutionData) -> io::Result<()> {
        let root_pid = data.pid.expect("child pid not set");

        attach(Pid::from_raw(root_pid))?;
        waitpid(Pid::from_raw(root_pid), None)?;
        setoptions(Pid::from_raw(root_pid), *PTRACE_OPTIONS)?;
        cont(Pid::from_raw(root_pid), None)?;

        Ok(())
    }

    fn on_wakeup(&mut self, _: &ExecutionSettings, _: &mut ExecutionData) -> io::Result<WakeupAction> {
        Ok(WakeupAction::Continue)
    }

    fn on_execute_event(
        &mut self,
        _: &ExecutionSettings,
        data: &mut ExecutionData,
        status: &WaitStatus
    ) -> io::Result<WakeupAction> {
        if let WaitStatus::PtraceEvent(_, _, _) = status
            && kill(Pid::from_raw(data.pid.expect("pid should not be None")), None).is_ok() {
            cont(Pid::from_raw(data.pid.expect("pid should not be None")), None)?
        }

        Ok(WakeupAction::Continue)
    }

    fn on_post_execute(&mut self, _: &ExecutionSettings, _: &mut ExecutionData) -> io::Result<()> {
        Ok(())
    }
}
