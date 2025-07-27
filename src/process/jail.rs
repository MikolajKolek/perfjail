use enumset::{EnumSet, EnumSetType};
use libc::{pthread_attr_destroy, pthread_attr_init, pthread_attr_setdetachstate, pthread_attr_t, pthread_create, pthread_t, PTHREAD_CREATE_DETACHED};
use std::ffi::{c_int, CString, OsStr};
use std::os::fd::{BorrowedFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs, io, mem};

use crate::listener::perf::PerfListener;
use crate::listener::Listener;
use crate::listener::memory::MemoryLimitListener;
use crate::listener::time_limit::TimeLimitListener;
use crate::listener::ptrace::PtraceListener;
use crate::process::child::{clone_and_execute, JailedChild};
use crate::process::data::{ExecutionContext, ExecutionData, ExecutionSettings};
use crate::util::{cvt_no_errno, CYCLES_PER_SECOND};

/// A builder based on [`std::process::Command`] used to configure and spawn perfjail processes.
///
/// A default configuration can be generated using [`Perfjail::new`].
/// Additional builder methods allow the configuration to be changed (for example, by adding arguments) prior to spawning:
/// ```
/// use perfjail::process::Perfjail;
///
/// let result = Perfjail::new("sleep")
///         .arg("1")
///         .spawn()
///         .expect("failed to spawn child")
///         .run()
///         .expect("failed to execute process");
///
/// let execution_time = result.real_time;
/// ```
///
/// Unlike [`std::process::Command`], `Perfjail` cannot be used to spawn multiple
/// processes, as [`Perfjail::spawn`] consumes itself after it's called.
pub struct Perfjail<'a> {
    pub(crate) real_time_limit: Option<Duration>,
    pub(crate) user_time_limit: Option<Duration>,
    pub(crate) system_time_limit: Option<Duration>,
    pub(crate) user_system_time_limit: Option<Duration>,
    pub(crate) instruction_count_limit: Option<i64>,
    pub(crate) memory_limit_kibibytes: Option<u64>,
    pub(crate) executable_path: CString,
    pub(crate) args: Vec<CString>,
    pub(crate) working_dir: Option<PathBuf>,
    pub(crate) stdin_fd: Option<BorrowedFd<'a>>,
    pub(crate) stdout_fd: Option<BorrowedFd<'a>>,
    pub(crate) stderr_fd: Option<BorrowedFd<'a>>,
    pub(crate) features: EnumSet<Feature>,
}

/// Feature flags dictating sandboxing and performance measurement options for the child process.
#[allow(non_camel_case_types)]
#[derive(EnumSetType, Debug)]
pub enum Feature {
    /// Causes perfjail to measure the number of CPU instructions executed
    /// by the child program, allowing for much more accurate time measurement.
    /// Makes the [`ExecutionResult`](crate::process::ExecutionResult) returned by [`JailedChild::run`]
    /// include the [`instructions_used`](crate::process::execution_result::ExecutionResult::instructions_used)
    /// and [`measured_time`](crate::process::execution_result::ExecutionResult::measured_time) fields.
    PERF,
    /// Makes the [`ExecutionResult`](crate::process::ExecutionResult) returned by [`JailedChild::run`]
    /// include the [`real_time`](crate::process::execution_result::ExecutionResult::real_time),
    /// [`user_time`](crate::process::execution_result::ExecutionResult::user_time) and
    /// [`system_time`](crate::process::execution_result::ExecutionResult::system_time) fields.
    TIME_MEASUREMENT,
    /// Makes the [`ExecutionResult`](crate::process::ExecutionResult) returned by [`JailedChild::run`] include the
    /// [`memory_usage_kibibytes`](crate::process::execution_result::ExecutionResult::memory_usage_kibibytes),
    /// field.
    MEMORY_MEASUREMENT,
    PTRACE,
}

#[allow(dead_code)]
impl<'a> Perfjail<'a> {
    /// Constructs a new `Perfjail` for launching the program at path `program`, with the following default configuration:
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
    /// The search path to be used may be controlled by setting the `PATH` environment variable.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::Perfjail;
    ///
    /// Perfjail::new("sh")
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run sh");
    /// ```
    ///
    ///
    /// # Caveats
    ///
    /// [`Perfjail::new`] is only intended to accept the path of the program. If you pass a program
    /// path along with arguments like `Perfjail::new("ls -l")`, it will try to search for
    /// `ls -l` literally. The arguments need to be passed separately, such as via [`arg`](Perfjail::arg) or
    /// [`args`](Perfjail::args).
    ///
    /// ```no_run
    /// use perfjail::process::Perfjail;
    ///
    /// Perfjail::new("ls")
    ///     .arg("-l") // arg passed separately
    ///     .spawn()
    ///     .expect("ls command failed to start");
    /// ```
    pub fn new<S: AsRef<OsStr>>(program: S) -> Perfjail<'a> {
        Perfjail {
            real_time_limit: None,
            user_time_limit: None,
            system_time_limit: None,
            user_system_time_limit: None,
            instruction_count_limit: None,
            memory_limit_kibibytes: None,
            executable_path: CString::new(program.as_ref().as_encoded_bytes())
                .expect("Failed to convert program path to CString"),
            args: vec![CString::new(program.as_ref().as_encoded_bytes())
                .expect("Failed to convert program path to CString")],
            working_dir: None,
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
    /// # perfjail::process::Perfjail::new("sh")
    /// .arg("-C /path/to/repo")
    /// # ;
    /// ```
    ///
    /// usage would be:
    ///
    /// ```no_run
    /// # perfjail::process::Perfjail::new("sh")
    /// .arg("-C")
    /// .arg("/path/to/repo")
    /// # ;
    /// ```
    ///
    /// To pass multiple arguments see [`args`](Perfjail::args).
    ///
    /// Note that the argument is not passed through a shell but given literally to the program.
    /// This means that shell syntax like quotes, escaped characters, word splitting, glob patterns,
    /// variable substitution, etc. have no effect.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::Perfjail;
    ///
    /// Perfjail::new("ls")
    ///     .arg("-l")
    ///     .arg("-a")
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Perfjail<'a> {
        self.args.push(
            CString::new(arg.as_ref().as_encoded_bytes())
                .expect("Failed to convert program arg to CString"),
        );
        self
    }

    /// Adds multiple arguments to pass to the program.
    ///
    /// To pass a single argument, see [`arg`](Perfjail::arg).
    ///
    /// Note that the arguments are not passed through a shell but given
    /// literally to the program. This means that shell syntax like quotes,
    /// escaped characters, word splitting, glob patterns, variable substitution, etc.
    /// have no effect.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::Perfjail;
    ///
    /// Perfjail::new("ls")
    ///     .args(["-l", "-a"])
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn args<I, S>(mut self, args: I) -> Perfjail<'a>
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
    /// platform-specific and unstable, and it's recommended to use
    /// [`fs::canonicalize`] to get an absolute program path instead.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use perfjail::process::Perfjail;
    ///
    /// Perfjail::new("ls")
    ///     .current_dir("/bin")
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn current_dir<P: AsRef<Path>>(mut self, dir: P) -> Perfjail<'a> {
        self.working_dir = Some(PathBuf::from(dir.as_ref().as_os_str()));
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
    /// use perfjail::process::Perfjail;
    /// use std::fs::File;
    /// use std::os::fd::AsFd;
    ///
    /// let file = File::open("/dev/null").unwrap();
    ///
    /// Perfjail::new("ls")
    ///     .stdin(file.as_fd())
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn stdin<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> Perfjail<'a> {
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
    /// use perfjail::process::Perfjail;
    /// use std::fs::File;
    /// use std::os::fd::AsFd;
    ///
    /// let file = File::open("/dev/null").unwrap();
    ///
    /// Perfjail::new("ls")
    ///     .stdout(file.as_fd())
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn stdout<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> Perfjail<'a> {
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
    /// use perfjail::process::Perfjail;
    /// use std::fs::File;
    /// use std::os::fd::AsFd;
    ///
    /// let file = File::open("/dev/null").unwrap();
    ///
    /// Perfjail::new("ls")
    ///     .stderr(file.as_fd())
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn stderr<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> Perfjail<'a> {
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
    /// use perfjail::process::Feature::{PERF, MEMORY_MEASUREMENT};
    /// use perfjail::process::Perfjail;
    ///
    /// Perfjail::new("ls")
    ///     .features(PERF | MEMORY_MEASUREMENT)
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run ls");
    /// ```
    pub fn features<T: Into<EnumSet<Feature>>>(mut self, features: T) -> Perfjail<'a> {
        self.features.insert_all(features.into());
        self
    }

    /// Sets a limit on how much real time the child program can run for
    /// before it is killed and [`ExitStatus::TLE`](crate::process::ExitStatus::TLE) is
    /// returned as the exit status.
    ///
    /// Setting a real time limit also automatically enables the
    /// [`TIME_MEASUREMENT`](Feature::TIME_MEASUREMENT) feature flag,
    /// working the same way as if it was added using the [`features`](Perfjail::features) method.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use std::time::Duration;
    /// use perfjail::process::ExitStatus::TLE;
    /// use perfjail::process::Perfjail;
    ///
    /// let result = Perfjail::new("sleep")
    ///     .arg("1")
    ///     .real_time_limit(Duration::from_secs_f64(0.5))
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run sleep");
    /// ```
    pub fn real_time_limit(mut self, limit: Duration) -> Perfjail<'a> {
        self.real_time_limit = Some(limit);
        self
    }

    /// Sets a limit on how much user time the child program can run for
    /// before it is killed and [`ExitStatus::TLE`](crate::process::ExitStatus::TLE) is
    /// returned as the exit status.
    ///
    /// Setting a user time limit also automatically enables the
    /// [`TIME_MEASUREMENT`](Feature::TIME_MEASUREMENT) feature flag,
    /// working the same way as if it was added using the [`features`](Perfjail::features) method.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use std::time::Duration;
    /// use perfjail::process::ExitStatus::TLE;
    /// use perfjail::process::Perfjail;
    ///
    /// let result = Perfjail::new("sleep")
    ///     .arg("1")
    ///     .user_time_limit(Duration::from_secs_f64(0.5))
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run sleep");
    /// ```
    pub fn user_time_limit(mut self, limit: Duration) -> Perfjail<'a> {
        self.user_time_limit = Some(limit);
        self
    }

    /// Sets a limit on how much system time the child program can run for
    /// before it is killed and [`ExitStatus::TLE`](crate::process::ExitStatus::TLE) is
    /// returned as the exit status.
    ///
    /// Setting a system time limit also automatically enables the
    /// [`TIME_MEASUREMENT`](Feature::TIME_MEASUREMENT) feature flag,
    /// working the same way as if it was added using the [`features`](Perfjail::features) method.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use std::time::Duration;
    /// use perfjail::process::ExitStatus::TLE;
    /// use perfjail::process::Perfjail;
    ///
    /// let result = Perfjail::new("sleep")
    ///     .arg("1")
    ///     .system_time_limit(Duration::from_secs_f64(0.5))
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run sleep");
    /// ```
    pub fn system_time_limit(mut self, limit: Duration) -> Perfjail<'a> {
        self.system_time_limit = Some(limit);
        self
    }

    /// Sets a limit on how much total user time and system time the child program can run for
    /// before it is killed and [`ExitStatus::TLE`](crate::process::ExitStatus::TLE) is
    /// returned as the exit status.
    ///
    /// Setting a user+system time limit also automatically enables the
    /// [`TIME_MEASUREMENT`](Feature::TIME_MEASUREMENT) feature flag,
    /// working the same way as if it was added using the [`features`](Perfjail::features) method.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use std::time::Duration;
    /// use perfjail::process::ExitStatus::TLE;
    /// use perfjail::process::Perfjail;
    ///
    /// let result = Perfjail::new("sleep")
    ///     .arg("1")
    ///     .user_system_time_limit(Duration::from_secs_f64(0.5))
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run sleep");
    /// ```
    pub fn user_system_time_limit(mut self, limit: Duration) -> Perfjail<'a> {
        self.user_system_time_limit = Some(limit);
        self
    }

    /// Sets a limit on how much measured time (as described in [`ExecutionResult::measured_time`](crate::process::ExecutionResult::measured_time)) can pass after the child program is executed
    /// before it is killed and [`ExitStatus::TLE`](crate::process::ExitStatus::TLE) is
    /// returned as the exit status.
    ///
    /// Setting a measured time limit also automatically enables the [`PERF`](Feature::PERF) feature flag,
    /// working the same way as if it was added using the [`features`](Perfjail::features) method.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use std::time::Duration;
    /// use perfjail::process::ExitStatus::TLE;
    /// use perfjail::process::Perfjail;
    ///
    /// let result = Perfjail::new("sleep")
    ///     .arg("1")
    ///     .measured_time_limit(Duration::from_secs_f64(0.5))
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run sleep");
    /// ```
    pub fn measured_time_limit(mut self, limit: Duration) -> Perfjail<'a> {
        self.instruction_count_limit =
            Some((limit.as_millis() * ((CYCLES_PER_SECOND / 1_000) as u128)) as i64);
        self = self.features(Feature::PERF);
        self
    }

    /// Sets a limit on how much memory (as described in
    /// [`ExecutionResult::memory_usage_kibibytes`](crate::process::ExecutionResult::memory_usage_kibibytes))
    /// the child program can use at its peak before it is killed and
    /// [`ExitStatus::TLE`](crate::process::ExitStatus::MLE) is returned as the exit status.
    ///
    /// Setting a measured time limit also automatically enables the [`MEMORY_MEASUREMENT`](Feature::MEMORY_MEASUREMENT)
    /// feature flag, working the same way as if it was added using the [`features`](Perfjail::features) method.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use std::time::Duration;
    /// use perfjail::process::ExitStatus::TLE;
    /// use perfjail::process::Perfjail;
    ///
    /// let result = Perfjail::new("sleep")
    ///     .arg("1")
    ///     .memory_limit_kibibytes(8192) // 8 MB
    ///     .spawn()
    ///     .expect("failed to spawn child")
    ///     .run()
    ///     .expect("failed to run sleep");
    /// ```
    pub fn memory_limit_kibibytes(mut self, limit: u64) -> Perfjail<'a> {
        self.memory_limit_kibibytes = Some(limit);
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
    /// use perfjail::process::Perfjail;
    ///
    /// Perfjail::new("ls")
    ///     .spawn()
    ///     .expect("failed to spawn child process");
    /// ```
    pub fn spawn(self) -> io::Result<JailedChild<'a>> {
        let listeners: Vec<Box<dyn Listener>> = self
            .features
            .iter()
            .map(|feature| match feature {
                Feature::PERF => Box::new(PerfListener::new()) as Box<dyn Listener>,
                Feature::TIME_MEASUREMENT => Box::new(TimeLimitListener::new()),
                Feature::MEMORY_MEASUREMENT => Box::new(MemoryLimitListener::new()),
                Feature::PTRACE => Box::new(PtraceListener::new()),
            })
            .collect();

        let mut context = Box::new(ExecutionContext {
            settings: ExecutionSettings::new(self),
            data: ExecutionData::new(),
            listeners,
        });

        unsafe {
            let mut attr: pthread_attr_t = mem::zeroed();
            let mut thread: pthread_t = mem::zeroed();
            cvt_no_errno(pthread_attr_init(&mut attr as _))?;
            cvt_no_errno(pthread_attr_setdetachstate(&mut attr as _, PTHREAD_CREATE_DETACHED))?;
            cvt_no_errno(
                pthread_create(&mut thread, &attr, clone_and_execute, (&mut *context as *mut ExecutionContext) as _)
            )?;
            cvt_no_errno(pthread_attr_destroy(&mut attr as _))?;

            context.data.child_ready_barrier.wait();

            assert_ne!(context.data.raw_pid_fd, -1);
            context.data.pid_fd = Some(OwnedFd::from_raw_fd(context.data.raw_pid_fd));
            context.data.pid = Some(
                fs::read_to_string(format!("/proc/self/fdinfo/{}", context.data.raw_pid_fd))
                    .expect("The pid_fd does not exist")
                    .split("\n")
                    .find(|line| { line.contains("Pid:") })
                    .expect("The file descriptor is not a pidfd")
                    .split_whitespace()
                    .nth(1)
                    .expect("The file descriptor is not a valid pidfd")
                    .trim()
                    .parse::<c_int>()
                    .expect("The pid is not valid")
            );
        }

        Ok(JailedChild::new(context))
    }
}
