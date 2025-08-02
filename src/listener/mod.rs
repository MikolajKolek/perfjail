use crate::listener::WakeupAction::{Continue, Kill};
use crate::process::data::{ExecutionContext, ExecutionSettings, ParentData};
use nix::sys::wait::WaitStatus;
use std::fmt::Debug;
use std::io;

pub(crate) mod perf;
pub(crate) mod time;
pub(crate) mod ptrace;
pub(crate) mod memory;

pub(crate) trait Listener: Debug {
    fn requires_timeout(
        &self, 
        settings: &ExecutionSettings
    ) -> bool;

    fn on_post_clone_child(
        &self,
        context: &ExecutionContext,
    ) -> nix::Result<()>;

    fn on_post_clone_parent(
        &self,
        context: &ExecutionContext,
        parent_data: &mut ParentData,
    ) -> io::Result<()>;

    fn on_wakeup(
        &self,
        context: &ExecutionContext,
        parent_data: &mut ParentData,
    ) -> io::Result<WakeupAction>;

    fn on_execute_event(
        &self,
        context: &ExecutionContext,
        parent_data: &mut ParentData,
        event: &WaitStatus
    ) -> io::Result<WakeupAction>;

    fn on_post_execute(
        &self,
        context: &ExecutionContext,
        parent_data: &mut ParentData,
    ) -> io::Result<()>;
}

#[derive(PartialEq, Eq, Debug)]
pub(crate) enum WakeupAction {
    Continue,
    Kill,
}

impl WakeupAction {
    pub(crate) fn combine(&self, other: WakeupAction) -> WakeupAction {
        if *self == Kill || other == Kill {
            Kill
        } else {
            Continue
        }
    }
}