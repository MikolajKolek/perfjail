use crate::process::ExecuteAction::{Continue, Kill};

pub(crate) mod child;
pub(crate) mod data;
pub(crate) mod error;
pub(crate) mod execution_result;
pub(crate) mod jail;

pub use self::child::JailedChild;
pub use self::execution_result::ExecutionResult;
pub use self::execution_result::ExitReason;
pub use self::execution_result::ExitStatus;
pub use self::jail::Feature;
pub use self::jail::Perfjail;

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
