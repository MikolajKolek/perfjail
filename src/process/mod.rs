use crate::process::ExecuteAction::{Continue, Kill};

pub mod child;
/// The executor used for running sio2jail
pub mod executor;
pub mod data;
pub mod error;
pub mod execution_result;

#[derive(PartialEq, Eq, Debug)]
pub(crate) enum ExecuteAction {
	Continue,
	Kill
}

impl ExecuteAction {
	fn preserve_kill(&self, other: ExecuteAction) -> ExecuteAction {
		if *self == Kill || other == Kill {
			Kill
		} else {
			Continue
		}
	}
}