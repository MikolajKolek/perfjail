use crate::listener::WakeupAction;
use crate::process::child::ChildState::{Reapable, Reaped};
use crate::process::data::ExecutionContext;
use crate::process::execution_result::{ExecutionResult, ExitReason};
use crate::util::{kill_pid, CHILD_STACK_SIZE};
use cvt::{cvt, cvt_r};
use libc::{clone, id_t, pid_t, siginfo_t, waitid, waitpid, CLD_DUMPED, CLD_EXITED, CLD_KILLED, CLONE_PIDFD, CLONE_VFORK, CLONE_VM, P_PID, SIGCHLD, WEXITED, WNOHANG, WNOWAIT, WSTOPPED};
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use nix::unistd::{chdir, close, dup2_stderr, dup2_stdin, dup2_stdout, execvp};
use std::ffi::{c_int, c_void};
use std::io;
use std::mem::zeroed;
use std::os::fd::{AsFd, AsRawFd};
use std::ptr::null_mut;
use std::sync::{Mutex, Once};

enum ChildState {
    Reapable { pid: pid_t },
    Reaped
}

/// Representation of a perfjail child process that's waiting to be run, running or exited.
///
/// This structure is used to represent and manage child processes. A child
/// process is created via the [`Perfjail`](crate::process::Perfjail) struct, which configures the
/// spawning process and can itself be constructed using a builder-style interface.
///
/// Calling [`run`](JailedChild::run) will make the parent process wait until the child has
/// exited before continuing.
///
/// Similarly to [`std::process::Command`], dropping the child without waiting for [`run`](JailedChild::run)
/// to finish at least once will not free its resources and will leave it hanging as a zombie process.
///
/// # Examples
///
/// ```
/// use perfjail::process::ExitReason::Exited;
/// use perfjail::process::ExitStatus::OK;
/// use perfjail::process::Perfjail;
///
/// let mut child = Perfjail::new("echo")
///     .arg("test")
///     .spawn()
///     .expect("failed to execute child");
///
/// let result = child.run().expect("failed to wait on child");
///
/// assert!(matches!(result.exit_reason, Exited { exit_status: 0 }));
/// ```
pub struct JailedChild<'a> {
    child_internals: Mutex<ChildInternals<'a>>,
    child_state: Mutex<ChildState>,
    run_once: Once
}

struct ChildInternals<'a> {
    context: Box<ExecutionContext<'a>>,
    run_error: Option<io::Error>,
}

unsafe impl Sync for JailedChild<'_> {}
unsafe impl Send for JailedChild<'_> {}

impl JailedChild<'_> {
    pub(crate) fn new(context: Box<ExecutionContext>) -> JailedChild {
        let pid = context.data.pid.expect("pid not set");

        JailedChild {
            child_internals: Mutex::new(ChildInternals { context, run_error: None }),
            child_state: Mutex::new(Reapable { pid }),
            run_once: Once::new(),
        }
    }

    /// Runs the child process and waits for it to exit completely, returning the result that it
    /// exited with. This function will continue to have the same return value after it has been
    /// called at least once.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::{ExitReason, Perfjail};
    ///
    /// let mut jail = Perfjail::new("ls");
    /// if let Ok(mut child) = jail.spawn() {
    ///     let result = child.run().expect("perfjail wasn't running");
    ///     assert_eq!(result.exit_reason, ExitReason::Exited { exit_status: 0 });
    /// } else {
    ///     panic!("ls command didn't start");
    /// }
    /// ```
    pub fn run(&self) -> io::Result<ExecutionResult> {
        let mut child_internals = self.child_internals.lock()
            .expect("Failed to lock child_internals");
        self.run_once.call_once(|| child_internals.run_saving_result(&self.child_state));

        if let Some(e) = (&mut child_internals).run_error.take() {
            Err(e)
        } else {
            Ok(child_internals.context.data.execution_result.clone())
        }
    }

    /// Forces the child process to exit. If the child has already exited, `Ok(())` is returned.
    ///
    /// This is equivalent to sending a SIGKILL signal.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::{ExitReason, Perfjail};
    ///
    /// let jail = Perfjail::new("yes");
    /// if let Ok(child) = jail.spawn() {
    ///     child.kill().expect("perfjail couldn't be killed");
    ///     assert_eq!(child.run().unwrap().exit_reason, ExitReason::Killed { signal: 9 });
    /// } else {
    ///     panic!("yes command didn't start");
    /// }
    /// ```
    pub fn kill(&self) -> io::Result<()> {
        let child_state = self.child_state.lock().expect("Failed to lock child_state");

        if let Reapable { pid } = *child_state {
            kill_pid(pid)?;
        }

        Ok(())
    }
}

impl ChildInternals<'_> {
    pub(crate) fn run_saving_result(&mut self, child_state: &Mutex<ChildState>) {
        if let Err(e) = self.run() {
            _ = self.run_error.insert(e);
        }

        unsafe {
            kill_pid(self.context.data.pid.expect("pid not set")).expect("Failed to kill child process");

            let mut child_state = child_state.lock().expect("Failed to lock pid_valid");
            *child_state = Reaped;
            drop(child_state);

            cvt_r(|| { waitpid(
                self.context.data.pid.unwrap() as id_t as pid_t,
                null_mut::<c_int>(),
                WNOHANG,
            )}).expect("Failed to clean up child process");
        }
    }

    fn run(&mut self) -> io::Result<()> {
        self.propagate_child_error()?;
        for listener in &mut self.context.listeners {
            listener.on_post_clone_parent(&self.context.settings, &mut self.context.data)?;
        }

        loop {
            let mut action = WakeupAction::Continue { next_wakeup: None };
            for listener in &mut self.context.listeners {
                action = action.combine(
                    listener.on_wakeup(&self.context.settings, &mut self.context.data)?
                );
            }

            if action == WakeupAction::Kill {
                kill_pid(self.context.data.pid.unwrap())?
            }

            let poll_pid_fd = PollFd::new(
                self.context.data.pid_fd.as_ref().unwrap().as_fd(),
                PollFlags::POLLIN,
            );
            let mut poll_fds = [poll_pid_fd];
            let poll_result = poll(
                &mut poll_fds,
                PollTimeout::try_from(action.next_wakeup().unwrap_or(-1)).unwrap(),
            );
            if let Err(e) = poll_result && (e == nix::errno::Errno::EINTR || e == nix::errno::Errno::EAGAIN) {
                continue;
            }

            let poll_result = poll_result?;
            if poll_result == 0 {
                // This means that one of the listeners' timeouts has finished,
                // and we need to call all the on_wakeup functions again
                continue;
            }

            let mut wait_info: siginfo_t = unsafe { zeroed() };
            unsafe {
                cvt_r(|| { waitid(
                    P_PID,
                    self.context.data.pid.unwrap() as id_t,
                    &mut wait_info as *mut siginfo_t,
                    WEXITED | WSTOPPED | WNOWAIT,
                )})?;
            }

            self.propagate_child_error()?;

            if wait_info.si_code == CLD_EXITED {
                self.context
                    .data
                    .execution_result
                    .set_exit_reason(ExitReason::Exited {
                        exit_status: unsafe { wait_info.si_status() },
                    });

                break;
            }
            if wait_info.si_code == CLD_KILLED || wait_info.si_code == CLD_DUMPED {
                self.context
                    .data
                    .execution_result
                    .set_exit_reason(ExitReason::Killed {
                        signal: unsafe { wait_info.si_status() },
                    });

                break;
            }
        }

        for listener in &mut self.context.listeners {
            listener.on_post_execute(&self.context.settings, &mut self.context.data)?;
        }

        self.propagate_child_error()?;
        Ok(())
    }

    fn propagate_child_error(&mut self) -> io::Result<()> {
        if let Some(e) = self.context.data.child_error.take() {
            Err(e)
        } else {
            Ok(())
        }
    }
}

pub(crate) extern "C" fn clone_and_execute(memory: *mut c_void) -> *mut c_void {
    unsafe {
        let context_ptr = memory as *mut ExecutionContext;
        let context = &mut (*context_ptr);

        let result = cvt(clone(
                execute_child,
                (context.data.child_stack.as_mut_ptr() as *mut c_void).add(CHILD_STACK_SIZE),
                CLONE_VM | CLONE_PIDFD | CLONE_VFORK | SIGCHLD,
                (&mut *context as *mut ExecutionContext) as *mut c_void,
                &mut context.data.raw_pid_fd as *mut c_int as *mut c_void,
        ));
        
        if let Err(e) = result {
            context.data.child_error = Some(e);
        }
        
        null_mut()
    }
}

extern "C" fn execute_child(memory: *mut c_void) -> c_int {
    let context_ptr = memory as *mut ExecutionContext;
    let context = unsafe { &mut (*context_ptr) };

    context.data.child_error = Some(execute_child_impl(context).unwrap_err());

    1
}

fn execute_child_impl(context: &mut ExecutionContext) -> io::Result<()> {
    context.data.clone_barrier.wait();
    
    context
        .listeners
        .iter_mut()
        .try_for_each(|listener| listener.on_post_clone_child(&context.settings, &context.data))?;

    if let Some(working_dir) = context.settings.working_dir.as_ref() {
        chdir(working_dir)?;
    }

    if let Some(stdin_fd) = context.settings.stdin_fd.as_ref() {
        dup2_stdin(stdin_fd)?;
        close(stdin_fd.as_raw_fd())?;
    }
    if let Some(stdout_fd) = context.settings.stdout_fd.as_ref() {
        dup2_stdout(stdout_fd)?;
        close(stdout_fd.as_raw_fd())?;
    }
    if let Some(stderr_fd) = context.settings.stderr_fd.as_ref() {
        dup2_stderr(stderr_fd)?;
        close(stderr_fd.as_raw_fd())?;
    }

    execvp(&context.settings.executable_path, &context.settings.args)?;

    // Execv returns only if it has failed, in which case the function returns the appropriate result
    unreachable!();
}
