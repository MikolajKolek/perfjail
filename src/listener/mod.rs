use crate::listener::WakeupAction::{Continue, Kill};
use crate::process::data::{ExecutionData, ExecutionSettings};
use std::fmt::Debug;
use std::io;
use nix::sys::wait::WaitStatus;

pub(crate) mod perf;
pub(crate) mod time_limit;
pub(crate) mod ptrace;
pub(crate) mod memory;

pub(crate) trait Listener: Debug {
    fn requires_timeout(&self, settings: &ExecutionSettings) -> bool;

    fn on_post_clone_child(
        &self,
        settings: &ExecutionSettings,
        data: &ExecutionData,
    ) -> io::Result<()>;

    fn on_post_clone_parent(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData) -> io::Result<()>;

    fn on_wakeup(
        &mut self,
        settings: &ExecutionSettings,
        data: &mut ExecutionData,
    ) -> io::Result<WakeupAction>;

    fn on_execute_event(
        &mut self,
        settings: &ExecutionSettings,
        data: &mut ExecutionData,
        event: &WaitStatus
    ) -> io::Result<WakeupAction>;

    fn on_post_execute(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData) -> io::Result<()>;
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