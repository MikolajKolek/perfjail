use std::fmt::Debug;

use crate::process::data::{ExecutionData, ExecutionSettings};
use crate::process::error::RunError;
use crate::process::ExecuteAction;

pub(crate) mod perf;
pub(crate) mod seccomp;

pub(crate) trait Listener: Debug {
    fn on_post_fork_child(
        &mut self,
        settings: &ExecutionSettings,
        data: &ExecutionData,
    ) -> Result<(), RunError>;

    fn on_post_fork_parent(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData);

    fn on_post_execute(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData);

    fn on_wakeup(
        &mut self,
        settings: &ExecutionSettings,
        data: &mut ExecutionData,
    ) -> (ExecuteAction, Option<i32>);
}
