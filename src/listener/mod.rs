use crate::listener::WakeupAction::{Continue, Kill};
use crate::process::data::{ExecutionData, ExecutionSettings};
use std::fmt::Debug;
use std::io;

pub(crate) mod perf;
pub(crate) mod seccomp;
pub(crate) mod time_limit;

pub(crate) trait Listener: Debug {
    fn requires_timeout(&self, settings: &ExecutionSettings) -> bool;

    fn on_post_clone_child(
        &mut self,
        settings: &ExecutionSettings,
        data: &ExecutionData,
    ) -> io::Result<()>;

    fn on_post_clone_parent(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData) -> io::Result<()>;

    fn on_wakeup(
        &mut self,
        settings: &ExecutionSettings,
        data: &mut ExecutionData,
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