use crate::process::ExecuteAction::{Continue, Kill};

pub(crate) mod child;
pub(crate) mod data;
pub(crate) mod error;
pub(crate) mod execution_result;
pub(crate) mod executor;

pub use self::child::Sio2jailChild;
pub use self::execution_result::ExecutionResult;
pub use self::execution_result::ExitReason;
pub use self::execution_result::ExitStatus;
pub use self::executor::Feature;
pub use self::executor::Sio2jailExecutor;

#[derive(PartialEq, Eq, Debug)]
pub(crate) enum ExecuteAction {
    Continue,
    Kill,
}

impl ExecuteAction {
    pub(crate) fn preserve_kill(&self, other: ExecuteAction) -> ExecuteAction {
        if *self == Kill || other == Kill {
            Kill
        } else {
            Continue
        }
    }
}
