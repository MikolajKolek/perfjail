use std::fmt::Debug;

use crate::process::data::{ExecutionData, ExecutionSettings};
use crate::process::ExecuteAction;

pub(crate) mod perf;

pub(crate) trait Listener: Debug {
	fn on_post_fork_child(&mut self, settings: &ExecutionSettings, data: &ExecutionData);

	fn on_post_fork_parent(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData);

	fn on_post_execute(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData);
	
	fn on_wakeup(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData) -> (ExecuteAction, Option<i32>);
}