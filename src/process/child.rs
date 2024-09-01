use cvt::cvt;
use libc::{
    id_t, kill, pid_t, siginfo_t, waitid, waitpid, CLD_DUMPED, CLD_EXITED, CLD_KILLED, P_PID,
    SIGKILL, WEXITED, WNOHANG, WNOWAIT, WSTOPPED,
};
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use nix::unistd::{chdir, close, dup2, execvp};
use std::any::Any;
use std::cmp::min;
use std::ffi::{c_int, c_void};
use std::io;
use std::mem::zeroed;
use std::os::fd::{AsFd, AsRawFd};
use std::path::PathBuf;
use std::ptr::null_mut;

use crate::process::data::ExecutionContext;
use crate::process::error::RunError;
use crate::process::execution_result::{ExecutionResult, ExitReason};
use crate::process::ExecuteAction::{Continue, Kill};

/// Representation of a perfjail child process that's waiting to be run, running or exited.
///
/// This structure is used to represent and manage child processes. A child
/// process is created via the [`Perfjail`](crate::process::Perfjail) struct, which configures the
/// spawning process and can itself be constructed using a builder-style
/// interface.
///
/// If the `JailedChild` is dropped, the child process is terminated.
///
/// Calling [`run`](JailedChild::run) will make
/// the parent process wait until the child has exited before
/// continuing.
///
/// # Examples
///
/// ```should_panic
/// use perfjail::process::ExitReason::Exited;
/// use perfjail::process::ExitStatus::OK;
/// use perfjail::process::Perfjail;
///
/// let mut child = Perfjail::new("/bin/cat")
///     .arg("file.txt")
///     .spawn()
///     .expect("failed to execute child");
///
/// let result = child.run().expect("failed to wait on child");
///
/// assert!(matches!(result.exit_reason, Exited { exit_status: 0 }));
/// ```
pub struct JailedChild<'a> {
    context: Box<ExecutionContext<'a>>,
    child_stack: Box<dyn Any>,
}

impl JailedChild<'_> {
    pub(crate) fn new(context: Box<ExecutionContext>, child_stack: *mut c_void) -> JailedChild {
        JailedChild {
            context,
            child_stack: unsafe { Box::from_raw(child_stack) },
        }
    }

    /// Runs the child process and waits for it to exit completely, returning the result that it
    /// exited with. After the function returns, the [`JailedChild`] object is consumed.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::Perfjail;
    ///
    /// let mut jail = Perfjail::new("ls");
    /// if let Ok(mut child) = jail.spawn() {
    ///     child.run().expect("perfjail wasn't running");
    ///     println!("Perfjail has finished its execution!");
    /// } else {
    ///     println!("ls command didn't start");
    /// }
    /// ```
    pub fn run(mut self) -> Result<ExecutionResult, RunError> {
        self.context.listeners.iter_mut().for_each(|listener| {
            listener.on_post_fork_parent(&self.context.settings, &mut self.context.data)
        });

        loop {
            let mut timeout: Option<i32> = None;
            let mut action = Continue;
            self.context.listeners.iter_mut().for_each(|listener| {
                let (listener_action, listener_timeout) =
                    listener.on_wakeup(&self.context.settings, &mut self.context.data);
                action = action.preserve_kill(listener_action);

                if let Some(mut timeout) = &timeout {
                    timeout = min(timeout, listener_timeout.unwrap_or(i32::MAX));
                } else {
                    timeout = listener_timeout;
                }
            });

            if action == Kill {
                self.kill()?
            }

            let poll_pid_fd = PollFd::new(
                self.context.data.pid_fd.as_ref().unwrap().as_fd(),
                PollFlags::POLLIN,
            );
            let mut poll_fds = [poll_pid_fd];
            let poll_result = poll(
                &mut poll_fds,
                PollTimeout::try_from(timeout.unwrap_or(-1)).unwrap(),
            );
            if poll_result.is_err() && poll_result.unwrap_err() == nix::errno::Errno::EINTR {
                continue;
            }
            
            let poll_result = poll_result.unwrap();
            if poll_result == 0 {
                // This means that one of the listeners' timeouts has finished, and we need to call all the on_wakeup functions again
                continue;
            }

            let mut wait_info: siginfo_t = unsafe { zeroed() };
            unsafe {
                _ = waitid(
                    P_PID,
                    self.context.data.pid.unwrap() as id_t,
                    &mut wait_info as *mut siginfo_t,
                    WEXITED | WSTOPPED | WNOWAIT,
                );
            }

            if self.context.data.child_error.is_some() {
                return Err(self.context.data.child_error.take().unwrap());
            }

            if wait_info.si_code == CLD_EXITED
                || wait_info.si_code == CLD_KILLED
                || wait_info.si_code == CLD_DUMPED
            {
                unsafe {
                    if wait_info.si_code == CLD_EXITED {
                        self.context
                            .data
                            .execution_result
                            .set_exit_result(ExitReason::Exited {
                                exit_status: wait_info.si_status(),
                            });
                    } else {
                        self.context
                            .data
                            .execution_result
                            .set_exit_result(ExitReason::Killed {
                                signal: wait_info.si_status(),
                            });
                    }
                }

                break;
            }
        }

        self.context.listeners.iter_mut().for_each(|listener| {
            listener.on_post_execute(&self.context.settings, &mut self.context.data)
        });

        Ok(self.context.data.execution_result.clone())
    }

    /// Forces the child process to exit. If the child has already exited, `Ok(())` is returned.
    ///
    /// This is equivalent to sending a SIGKILL signal.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use perfjail::process::Perfjail;
    ///
    /// let mut jail = Perfjail::new("yes");
    /// if let Ok(mut child) = jail.spawn() {
    ///     child.kill().expect("perfjail couldn't be killed");
    /// } else {
    ///     println!("yes command didn't start");
    /// }
    /// ```
    pub fn kill(&mut self) -> io::Result<()> {
        unsafe {
            // The pid is guaranteed to still be valid, even if the child was already killed, as the child process hasn't yet been waited on, and won't be until it's dropped
            cvt(kill(self.context.data.pid.unwrap(), SIGKILL)).map(|_| ())
        }
    }
}

impl Drop for JailedChild<'_> {
    fn drop(&mut self) {
        self.kill().expect("failed to kill child");

        unsafe {
            waitpid(
                self.context.data.pid.unwrap() as id_t as pid_t,
                null_mut::<c_int>(),
                WNOHANG,
            );
        }
    }
}

pub(crate) extern "C" fn execute_child(memory: *mut c_void) -> c_int {
    let context_ptr = memory as *mut ExecutionContext;
    let context = unsafe { &mut (*context_ptr) };

    context.data.child_error = Some(execute_child_impl(context).unwrap_err());

    1
}

fn execute_child_impl(context: &mut ExecutionContext) -> Result<(), RunError> {
    context
        .listeners
        .iter_mut()
        .try_for_each(|listener| listener.on_post_fork_child(&context.settings, &context.data))?;

    if context.settings.working_dir != PathBuf::new() {
        chdir(&context.settings.working_dir)?;
    }

    if let Some(stdin_fd) = context.settings.stdin_fd.as_ref() {
        dup2(stdin_fd.as_raw_fd(), 0)?;
        close(stdin_fd.as_raw_fd())?;
    }
    if let Some(stdout_fd) = context.settings.stdout_fd.as_ref() {
        dup2(stdout_fd.as_raw_fd(), 1 as c_int)?;
        close(stdout_fd.as_raw_fd())?;
    }
    if let Some(stderr_fd) = context.settings.stderr_fd.as_ref() {
        dup2(stderr_fd.as_raw_fd(), 2 as c_int)?;
        close(stderr_fd.as_raw_fd())?;
    }

    execvp(&context.settings.executable_path, &context.settings.args)?;

    // Execv returns only if it has failed, in which case the function returns the appropriate result
    unreachable!();
}
