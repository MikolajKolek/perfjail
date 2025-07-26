use std::cmp::min;
use std::fmt::Debug;
use std::io;
use crate::listener::WakeupAction::{Continue, Kill};
use crate::process::data::{ExecutionData, ExecutionSettings};

pub(crate) mod perf;
pub(crate) mod time_limit;
pub(crate) mod memory;

pub(crate) trait Listener: Debug {
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
    Continue { next_wakeup: Option<i32> },
    Kill,
}

impl WakeupAction {
    pub(crate) fn combine(&self, other: WakeupAction) -> WakeupAction {
        if *self == Kill || other == Kill {
            Kill
        } else {
            Continue { 
                next_wakeup: if self.next_wakeup().is_some() || other.next_wakeup().is_some() {
                    Some(min(self.next_wakeup().unwrap_or(i32::MAX), other.next_wakeup().unwrap_or(i32::MAX)))
                } else {
                    None
                }
            }
        }
    }
    
    pub(crate) fn next_wakeup(&self) -> Option<i32> {
        if let Continue { next_wakeup } = *self {
            next_wakeup
        } else {
            None
        }
    }
}