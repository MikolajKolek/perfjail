use std::io;
use crate::listener::{Listener, WakeupAction};
use crate::process::data::{ExecutionData, ExecutionSettings};

#[derive(Debug)]
pub(crate) struct SeccompListener {}

impl SeccompListener {
    pub(crate) fn new() -> SeccompListener {
        SeccompListener {}
    }
}

impl Listener for SeccompListener {
    fn on_post_clone_child(
        &mut self,
        _: &ExecutionSettings,
        _: &ExecutionData,
    ) -> io::Result<()> {
        Ok(())
    }

    fn on_post_clone_parent(&mut self, _: &ExecutionSettings, _: &mut ExecutionData) -> io::Result<()> {
        Ok(())
    }

    fn on_wakeup(
        &mut self,
        _: &ExecutionSettings,
        _: &mut ExecutionData,
    ) -> io::Result<WakeupAction> {
        Ok(WakeupAction::Continue { next_wakeup: None })
    }

    fn on_post_execute(&mut self, _: &ExecutionSettings, _: &mut ExecutionData) -> io::Result<()> {
        Ok(())
    }
}
