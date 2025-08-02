use crate::listener::{Listener, WakeupAction};
use crate::process::data::{ExecutionSettings, ExecutionContext, ParentData};
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

    fn on_post_clone_child(&self, _: &ExecutionContext) -> nix::Result<()> {
        Ok(())
    }

    fn on_post_clone_parent(&self, _: &ExecutionContext, parent_data: &mut ParentData) -> io::Result<()> {
        let root_pid = parent_data.pid;

        attach(Pid::from_raw(root_pid))?;
        waitpid(Pid::from_raw(root_pid), None)?;
        setoptions(Pid::from_raw(root_pid), *PTRACE_OPTIONS)?;
        cont(Pid::from_raw(root_pid), None)?;

        Ok(())
    }

    fn on_wakeup(&self, _: &ExecutionContext, _: &mut ParentData) -> io::Result<WakeupAction> {
        Ok(WakeupAction::Continue)
    }

    fn on_execute_event(&self, _: &ExecutionContext, parent_data: &mut ParentData, status: &WaitStatus) -> io::Result<WakeupAction> {
        if let WaitStatus::PtraceEvent(_, _, _) = status
            && kill(Pid::from_raw(parent_data.pid), None).is_ok() {
            cont(Pid::from_raw(parent_data.pid), None)?
        }

        Ok(WakeupAction::Continue)
    }

    fn on_post_execute(&self, _: &ExecutionContext, _: &mut ParentData) -> io::Result<()> {
        Ok(())
    }
}
