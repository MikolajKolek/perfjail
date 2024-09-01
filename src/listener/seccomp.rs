use crate::listener::Listener;
use crate::process::data::{ExecutionData, ExecutionSettings};
use crate::process::error::RunError;
use crate::process::ExecuteAction;

#[derive(Debug)]
pub(crate) struct SeccompListener {}

impl SeccompListener {
    pub(crate) fn new() -> SeccompListener {
        SeccompListener {}
    }
}

impl Listener for SeccompListener {
    fn on_post_fork_child(
        &mut self,
        _: &ExecutionSettings,
        _: &ExecutionData,
    ) -> Result<(), RunError> {
        Ok(())
    }

    fn on_post_fork_parent(&mut self, _: &ExecutionSettings, _: &mut ExecutionData) {}

    fn on_post_execute(&mut self, _: &ExecutionSettings, _: &mut ExecutionData) {}

    fn on_wakeup(
        &mut self,
        _: &ExecutionSettings,
        _: &mut ExecutionData,
    ) -> (ExecuteAction, Option<i32>) {
        (ExecuteAction::Continue, None)
    }
}
