use nix::errno::Errno;
use std::io;
use thiserror::Error;

/// Error returned by [`Sio2jailExecutor::spawn`]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SpawnError {}

/// Error returned by [`Sio2jailChild::run`]
#[derive(Error, Debug)]
pub enum RunError {
    /// Child errno
    #[error("Child errno: {0}")]
    ChildErrno(#[from] Errno),
    /// IO error
    #[error("IO error: {0}")]
    IOError(#[from] io::Error),
}
