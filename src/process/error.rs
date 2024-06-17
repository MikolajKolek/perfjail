use thiserror::Error;

/// Error returned by [`Sio2jailExecutor::spawn`]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SpawnError {
	
}

/// Error returned by [`Sio2jailChild::run`]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RunError {

}

