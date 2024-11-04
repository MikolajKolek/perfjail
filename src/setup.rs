use std::io;
use std::io::ErrorKind::NotFound;
use std::process::Command;

use crate::setup::PerfSetupError::{AuthenticationFailed, PkexecNotFound, SetupCommandFail};
use sysctl::{Sysctl, SysctlError};
use thiserror::Error;

/// Error returned by [`set_perf_up_temporarily`] and [`set_perf_up_permanently`].
#[derive(Error, Debug)]
pub enum PerfSetupError {
    /// Pkexec was not found.
    #[error("Pkexec was not found")]
    PkexecNotFound,
    /// Failed to elevate permissions using pkexec.
    #[error("Failed to elevate permissions using pkexec")]
    AuthenticationFailed,
    /// Failed to run setup commands.
    #[error("Failed to run setup commands: {0}")]
    SetupCommandFail(String),
    /// IO error.
    #[error("IO Error: {0}")]
    IoError(#[from] io::Error),
}

/// Checks if the Linux kernel parameters required for running perfjail with the [`PERF`](crate::process::jail::Feature::PERF) feature are set, returning true if they are and false if they aren't.
/// ```no_run
/// use perfjail::setup::test_perf;
///
/// // Verify that perf is properly set up
/// assert_eq!(test_perf().unwrap_or(false), true);
/// ```
/// # Errors
/// Returns a [`SysctlError`] if the `kernel.perf_event_paranoid` sysctl cannot be read or doesn't exist.
pub fn test_perf() -> Result<bool, SysctlError> {
    let ctl = sysctl::Ctl::new("kernel.perf_event_paranoid")?;
    let ctl_string = ctl.value_string()?;

    Ok(ctl_string == "-1")
}

/// Temporarily sets the Linux kernel parameters required for running perfjail with the [`PERF`](crate::process::jail::Feature::PERF) feature.
///
/// This setup does not persist across reboots. For that, see [`set_perf_up_permanently`].
/// ```no_run
/// use perfjail::setup::set_perf_up_temporarily;
///
/// // Temporarily set Linux up for using perfjail with perf
/// set_perf_up_temporarily().expect("failed to set up perf");
/// ```
/// # Errors
/// Returns a [`PerfSetupError`] if setting perf up failed.
pub fn set_perf_up_temporarily() -> Result<(), PerfSetupError> {
    pkexec_command("sysctl", vec!["-w", "kernel.perf_event_paranoid=-1"])
}

/// Permanently sets the Linux kernel parameters required for running perfjail with the [`PERF`](crate::process::jail::Feature::PERF) feature (this persists across reboots).
///
/// This is achieved by adding a line to `/etc/sysctl.conf`.
///
/// If you want to set the kernel parameters without persisting across reboots, see [`set_perf_up_temporarily`].
/// ```no_run
/// use perfjail::setup::set_perf_up_permanently;
///
/// // Permanently set Linux up for using perfjail with perf
/// set_perf_up_permanently().expect("failed to set up perf");
/// ```
/// # Errors
/// Returns a [`PerfSetupError`] if setting perf up failed.
pub fn set_perf_up_permanently() -> Result<(), PerfSetupError> {
    pkexec_command("bash", vec![
		"-c",
			"set -e;\
			sysctl -w kernel.perf_event_paranoid=-1;\
			echo -e \"\n# Settings required by perfjail:\nkernel.perf_event_paranoid = -1\" >> /etc/sysctl.conf;"
	])
}

fn pkexec_command(program: &str, args: Vec<&str>) -> Result<(), PerfSetupError> {
    let output = Command::new("pkexec").arg(program).args(args).output();

    if let Ok(output) = output {
        if output.status.code().is_none() {
            Err(SetupCommandFail(String::from(
                "the process was terminated by a signal",
            )))
        } else if output.status.code().unwrap() == 127 || output.status.code().unwrap() == 126 {
            Err(AuthenticationFailed)
        } else if !output.stderr.is_empty() {
            Err(SetupCommandFail(String::from_utf8(output.stderr).map_err(
                |_| SetupCommandFail(String::from("stderr is not valid UTF-8")),
            )?))
        } else if output.status.code().unwrap() != 0 {
            Err(SetupCommandFail(format!(
                "the process returned a non-zero return code: {}",
                output.status.code().unwrap()
            )))
        } else {
            Ok(())
        }
    } else {
        let error = output.unwrap_err();

        if error.kind() == NotFound {
            Err(PkexecNotFound)
        } else {
            Err(error.into())
        }
    }
}
