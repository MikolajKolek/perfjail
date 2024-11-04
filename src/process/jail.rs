use std::ffi::{c_int, c_void, CString, OsStr};
use std::io;
use std::os::fd::{BorrowedFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::Duration;

use cvt::cvt;
use enumset::{EnumSet, EnumSetType};
use libc::{clone, malloc, CLONE_PIDFD, CLONE_VM, SIGCHLD};

use crate::listener::perf::PerfListener;
use crate::listener::seccomp::SeccompListener;
use crate::listener::Listener;
use crate::process::child::{execute_child, JailedChild};
use crate::process::data::{ExecutionContext, ExecutionData, ExecutionSettings};
use crate::process::jail::Feature::PERF;
use crate::util::{CHILD_STACK_SIZE, CYCLES_PER_SECOND};

/// A builder based on [`std::process::Command`] used to configure and spawn perfjail processes.
///
/// A default configuration can be generated using [`PerfJail::new`].
/// Additional builder methods allow the configuration to be changed (for example, by adding arguments) prior to spawning:
/// ```
/// use perfjail::process::PerfJail;
///
/// let result = PerfJail::new("sleep")
///         .arg("1")
///         .spawn()
///         .expect("failed to spawn child")
///         .run()
///         .expect("failed to execute process");
///
/// let execution_time = result.real_time;
/// ```
///
/// Unlike [`std::process::Command`], `PerfJail` cannot be used to spawn multiple
/// processes, as [`PerfJail::spawn`] consumes itself after it's called.
pub struct PerfJail<'a> {
    pub(crate) real_time_limit: Option<Duration>,
    pub(crate) instruction_count_limit: Option<i64>,
    pub(crate) executable_path: CString,
    pub(crate) args: Vec<CString>,
    pub(crate) working_dir: PathBuf,
    pub(crate) stdin_fd: Option<BorrowedFd<'a>>,
    pub(crate) stdout_fd: Option<BorrowedFd<'a>>,
    pub(crate) stderr_fd: Option<BorrowedFd<'a>>,
    pub(crate) features: EnumSet<Feature>,
}

/// Feature flags dictating sandboxing and performance measurement options for the child process.
#[derive(EnumSetType, Debug)]
pub enum Feature {
    /// Causes perfjail to measure the number of CPU instructions executed
    /// by the child program, allowing for much more accurate time measurement.
    /// Makes the [`ExecutionResult`](crate::process::ExecutionResult) returned by [`PerfJail::run`]
    /// include the [`instructions_used`](crate::process::execution_result::ExecutionResult::instructions_used)
    /// and [`measured_time`](crate::process::execution_result::ExecutionResult::measured_time) fields.
    PERF,
    SECCOMP,
}

#[allow(dead_code)]
impl<'a> PerfJail<'a> {
    /// Constructs a new `PerfJail` for launching the program at path `program`, with the following default configuration:
    ///
    /// - No arguments to the program
    /// - Inherit the current process’s environment
    /// - Inherit the current process’s working directory
    /// - Inherit stdin/stdout/stderr
    /// - Don't enable any features and don't set and time limits
    ///
    /// Builder methods are provided to change these defaults and otherwise configure the process.
    ///
    /// If `program` is not an absolute path, the `PATH` will be searched in an OS-defined way.
    ///
    /// The search path to be used may be controlled by setting the `PATH` environment variable on the Command.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::PerfJail;
    ///
    /// PerfJail::new("sh")
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run sh");
    /// ```
    pub fn new<S: AsRef<OsStr>>(program: S) -> PerfJail<'a> {
        PerfJail {
            real_time_limit: None,
            instruction_count_limit: None,
            executable_path: CString::new(program.as_ref().as_encoded_bytes())
                .expect("Failed to convert program path to CString"),
            args: vec![CString::new(program.as_ref().as_encoded_bytes())
                .expect("Failed to convert program path to CString")],
            working_dir: PathBuf::new(),
            stdin_fd: None,
            stdout_fd: None,
            stderr_fd: None,
            features: EnumSet::new(),
        }
    }

    /// Adds an argument to pass to the program.
    ///
    /// Only one argument can be passed per use. So instead of:
    ///
    /// ```no_run
    /// # perfjail::process::PerfJail::new("sh")
    /// .arg("-C /path/to/repo")
    /// # ;
    /// ```
    ///
    /// usage would be:
    ///
    /// ```no_run
    /// # perfjail::process::PerfJail::new("sh")
    /// .arg("-C")
    /// .arg("/path/to/repo")
    /// # ;
    /// ```
    ///
    /// To pass multiple arguments see [`args`](PerfJail::args).
    ///
    /// Note that the argument is not passed through a shell, but given literally to the program.
    /// This means that shell syntax like quotes, escaped characters, word splitting, glob patterns,
    /// variable substitution, etc. have no effect.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::PerfJail;
    ///
    /// PerfJail::new("ls")
    ///     .arg("-l")
    ///     .arg("-a")
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> PerfJail<'a> {
        self.args.push(
            CString::new(arg.as_ref().as_encoded_bytes())
                .expect("Failed to convert program arg to CString"),
        );
        self
    }

    /// Adds multiple arguments to pass to the program.
    ///
    /// To pass a single argument see [`arg`](PerfJail::arg).
    ///
    /// Note that the arguments are not passed through a shell, but given
    /// literally to the program. This means that shell syntax like quotes,
    /// escaped characters, word splitting, glob patterns, variable substitution, etc.
    /// have no effect.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::PerfJail;
    ///
    /// PerfJail::new("ls")
    ///     .args(["-l", "-a"])
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn args<I, S>(mut self, args: I) -> PerfJail<'a>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self = self.arg(arg.as_ref());
        }
        self
    }

    /// Sets the working directory for the child process.
    ///
    /// # Platform-specific behavior
    ///
    /// If the program path is relative (e.g., `"./script.sh"`), it's ambiguous
    /// whether it should be interpreted relative to the parent's working
    /// directory or relative to `current_dir`. The behavior in this case is
    /// platform specific and unstable, and it's recommended to use
    /// [`std::fs::canonicalize`] to get an absolute program path instead.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::PerfJail;
    ///
    /// PerfJail::new("ls")
    ///     .current_dir("/bin")
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn current_dir<P: AsRef<Path>>(mut self, dir: P) -> PerfJail<'a> {
        self.working_dir = PathBuf::from(dir.as_ref().as_os_str());
        self
    }

    /// Sets the file descriptor for the child process’s standard input (stdin).
    ///
    /// If this function is not called, child stdin is inherited from the parent process.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::PerfJail;
    /// use std::fs::File;
    /// use std::os::fd::AsFd;
    ///
    /// let file = File::open("/dev/null").unwrap();
    ///
    /// PerfJail::new("ls")
    ///     .stdin(file.as_fd())
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn stdin<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> PerfJail<'a> {
        self.stdin_fd = Some(fd.into());
        self
    }

    /// Sets the file descriptor for the child process’s standard output (stdout).
    ///
    /// If this function is not called, child stdout is inherited from the parent process.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::PerfJail;
    /// use std::fs::File;
    /// use std::os::fd::AsFd;
    ///
    /// let file = File::open("/dev/null").unwrap();
    ///
    /// PerfJail::new("ls")
    ///     .stdout(file.as_fd())
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn stdout<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> PerfJail<'a> {
        self.stdout_fd = Some(fd.into());
        self
    }

    /// Sets the file descriptor for the child process’s standard error (stderr).
    ///
    /// If this function is not called, child stderr is inherited from the parent process.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::PerfJail;
    /// use std::fs::File;
    /// use std::os::fd::AsFd;
    ///
    /// let file = File::open("/dev/null").unwrap();
    ///
    /// PerfJail::new("ls")
    ///     .stderr(file.as_fd())
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn stderr<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> PerfJail<'a> {
        self.stderr_fd = Some(fd.into());
        self
    }

    /// Adds feature flags to influence how program execution is sandboxed and measured.
    ///
    /// Multiple features can be added at once if they are separated by the `|` character.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use std::time::Duration;
    /// use perfjail::process::Feature::{PERF, SECCOMP};
    /// use perfjail::process::PerfJail;
    ///
    /// PerfJail::new("ls")
    ///     .features(PERF | SECCOMP)
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn features<T: Into<EnumSet<Feature>>>(mut self, features: T) -> PerfJail<'a> {
        self.features.insert_all(features.into());
        self
    }

    /// Sets a limit on how much real time can pass after the child program is executed
    /// before it is killed and [`ExitStatus::TLE`](crate::process::ExitStatus::TLE) is
    /// returned as the exit status.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use std::time::Duration;
    /// use perfjail::process::ExitStatus::TLE;
    /// use perfjail::process::PerfJail;
    ///
    /// let result = PerfJail::new("sleep")
    ///     .arg("1")
    ///     .real_time_limit(Duration::from_secs_f64(0.5))
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run sleep");
    /// ```
    pub fn real_time_limit(mut self, limit: Duration) -> PerfJail<'a> {
        self.real_time_limit = Some(limit);
        self
    }

    /// Sets a limit on how much measured time (as described in [`ExecutionResult::measured_time`](crate::process::ExecutionResult::measured_time)) can pass after the child program is executed
    /// before it is killed and [`ExitStatus::TLE`](crate::process::ExitStatus::TLE) is
    /// returned as the exit status.
    ///
    /// Setting a measured time limit also automatically enables the [`PERF`](PERF) feature flag, working the same way as if it was added using the [`features`](PerfJail::features) method.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use std::time::Duration;
    /// use perfjail::process::ExitStatus::TLE;
    /// use perfjail::process::PerfJail;
    ///
    /// let result = PerfJail::new("sleep")
    ///     .arg("1")
    ///     .measured_time_limit(Duration::from_secs_f64(0.5))
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run sleep");
    /// ```
    pub fn measured_time_limit(mut self, limit: Duration) -> PerfJail<'a> {
        self.instruction_count_limit =
            Some((limit.as_millis() * ((CYCLES_PER_SECOND / 1_000) as u128)) as i64);
        self = self.features(PERF);
        self
    }

    /// Spawns the child process used for the execution of the program, returning a handle to it.
    ///
    /// Note that this does not start the execution of the program and instead just spawns the child process preparing for its execution, waiting for it to start until [`JailedChild::run`](JailedChild::run) is run.
    ///
    /// By default, stdin, stdout and stderr are inherited from the parent.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::PerfJail;
    ///
    /// PerfJail::new("ls")
    ///     .spawn()
    ///     .expect("failed to spawn child process");
    /// ```
    pub fn spawn(self) -> io::Result<JailedChild<'a>> {
        let listeners = self
            .features
            .iter()
            .map(|feature| match feature {
                PERF => Box::new(PerfListener::new()) as Box<dyn Listener>,
                Feature::SECCOMP => Box::new(SeccompListener::new()) as Box<dyn Listener>,
            })
            .collect();

        let mut context = Box::new(ExecutionContext {
            settings: ExecutionSettings::new(self),
            data: ExecutionData::new(),
            listeners,
        });

        let child_stack = unsafe { malloc(CHILD_STACK_SIZE) };
        unsafe {
            let mut pid_fd: c_int = -1;

            context.data.pid = Some(
                cvt(clone(
                    execute_child,
                    child_stack.add(CHILD_STACK_SIZE),
                    CLONE_VM | CLONE_PIDFD | SIGCHLD,
                    (&mut *context as *mut ExecutionContext) as *mut c_void,
                    &mut pid_fd as *mut c_int as *mut c_void,
                ))
                .unwrap(),
            );

            context.data.pid_fd = Some(OwnedFd::from_raw_fd(pid_fd));
        }

        Ok(JailedChild::new(context, child_stack))
    }
}
