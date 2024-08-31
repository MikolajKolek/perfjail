use nix::errno::Errno;
use std::io;
use thiserror::Error;

/// Error returned by [`PerfJail::spawn`]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SpawnError {}

/// Error returned by [`JailedChild::run`].
#[derive(Error, Debug)]
pub enum RunError {
    /// Child errno
    #[error("Child errno: {0}")]
    ChildErrno(#[from] Errno),
    /// IO error
    #[error("IO error: {0}")]
    IOError(#[from] io::Error),
}
