use std::ffi::{c_int, c_void, CString, OsStr};
use std::io;
use std::os::fd::{BorrowedFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::Duration;

use cvt::cvt;
use enumset::{EnumSet, EnumSetType};
use libc::{clone, CLONE_PIDFD, CLONE_VM, malloc, SIGCHLD};

use crate::listener::Listener;
use crate::listener::perf::PerfListener;
use crate::listener::seccomp::SeccompListener;
use crate::process::child::{execute_child, Sio2jailChild};
use crate::process::data::{ExecutionContext, ExecutionData, ExecutionSettings};
use crate::process::executor::Feature::PERF;
use crate::util::{CHILD_STACK_SIZE, CYCLES_PER_SECOND};

/// A builder based on [`std::process::Command`] used to configure and spawn libsio2jail processes.
///
/// A default configuration can be generated using [`Sio2jailExecutor::new`].
/// Additional builder methods allow the configuration to be changed (for example, by adding arguments) prior to spawning:
/// ```
/// use libsio2jail::process::Sio2jailExecutor;
///
/// let result = Sio2jailExecutor::new("sleep")
///         .arg("1")
///         .spawn()
///         .expect("failed to spawn child")
///         .run()
///         .expect("failed to execute process");
///
/// let execution_time = result.real_time;
/// ```
///
/// Unlike [`std::process::Command`], `Sio2jailExecutor` cannot be used to spawn multiple
/// processes, as [`Sio2jailExecutor::spawn`] consumes itself after it's called.
pub struct Sio2jailExecutor<'a> {
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
	/// Causes libsio2jail to measure the number of CPU instructions executed
	/// by the child program, allowing for much more accurate time measurement.
	/// Causes the [ExecutionResult](crate::process::ExecutionResult) returned by [`Sio2jailChild::run`]
	/// to include the [instructions_used](crate::process::execution_result::ExecutionResult::instructions_used)
	/// and [measured_time](crate::process::execution_result::ExecutionResult::measured_time) fields.
	PERF,
	SECCOMP
}

#[allow(dead_code)]
impl<'a> Sio2jailExecutor<'a> {
	/// Constructs a new `Sio2jailExecutor` for launching the program at path `program`, with the following default configuration:
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
	/// use libsio2jail::process::Sio2jailExecutor;
	///
	/// Sio2jailExecutor::new("sh")
 	///     .spawn()
 	///     .expect("failed to spawn child")
	///     .run()
	///     .expect("failed to run sh");
	/// ```
	pub fn new<S: AsRef<OsStr>>(program: S) -> Sio2jailExecutor<'a> {
		Sio2jailExecutor {
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
	/// # libsio2jail::process::Sio2jailExecutor::new("sh")
	/// .arg("-C /path/to/repo")
	/// # ;
	/// ```
	///
	/// usage would be:
	///
	/// ```no_run
	/// # libsio2jail::process::Sio2jailExecutor::new("sh")
	/// .arg("-C")
	/// .arg("/path/to/repo")
	/// # ;
	/// ```
	///
	/// To pass multiple arguments see [`args`](Sio2jailExecutor::args).
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
	/// use libsio2jail::process::Sio2jailExecutor;
 	///
	/// Sio2jailExecutor::new("ls")
 	///     .arg("-l")
	///     .arg("-a")
	///     .spawn()
	///     .expect("failed to spawn child")
	///     .run()
	///     .expect("failed to run ls");
	/// ```
	pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Sio2jailExecutor<'a> {
		self.args.push(
			CString::new(arg.as_ref().as_encoded_bytes())
				.expect("Failed to convert program arg to CString"),
		);
		self
	}

	/// Adds multiple arguments to pass to the program.
	///
	/// To pass a single argument see [`arg`](Sio2jailExecutor::arg).
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
	/// use libsio2jail::process::Sio2jailExecutor;
	///
	/// Sio2jailExecutor::new("ls")
	///     .args(["-l", "-a"])
	///     .spawn()
	///     .expect("failed to spawn child")
	///     .run()
	///     .expect("failed to run ls");
	/// ```
	pub fn args<I, S>(mut self, args: I) -> Sio2jailExecutor<'a>
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
	/// use libsio2jail::process::Sio2jailExecutor;
	///
	/// Sio2jailExecutor::new("ls")
	///     .current_dir("/bin")
	///     .spawn()
	///     .expect("failed to spawn child")
	///     .run()
	///     .expect("failed to run ls");
	/// ```
	pub fn current_dir<P: AsRef<Path>>(mut self, dir: P) -> Sio2jailExecutor<'a> {
		self.working_dir = PathBuf::from(dir.as_ref().as_os_str());
		self
	}

	/// Sets the file descriptor for the child process’s standard input (stdin)
	///
	/// If this function is not called, child stdin is inherited from the parent process
	///
	/// # Examples
	///
	/// Basic usage:
	///
	/// ```
	/// use libsio2jail::process::Sio2jailExecutor;
	/// use std::fs::File;
	/// use std::os::fd::AsFd;
	///
	/// let file = File::open("/dev/null").unwrap();
	///
	/// Sio2jailExecutor::new("ls")
	///     .stdin(file.as_fd())
	///     .spawn()
	///     .expect("failed to spawn child")
	///     .run()
	///     .expect("failed to run ls");
	/// ```
	pub fn stdin<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> Sio2jailExecutor<'a> {
		self.stdin_fd = Some(fd.into());
		self
	}

	/// Sets the file descriptor for the child process’s standard output (stdout)
	///
	/// If this function is not called, child stdout is inherited from the parent process
	///
	/// # Examples
	///
	/// Basic usage:
	///
	/// ```
	/// use libsio2jail::process::Sio2jailExecutor;
	/// use std::fs::File;
	/// use std::os::fd::AsFd;
	///
	/// let file = File::open("/dev/null").unwrap();
	///
	/// Sio2jailExecutor::new("ls")
	///     .stdout(file.as_fd())
	///     .spawn()
	///     .expect("failed to spawn child")
	///     .run()
	///     .expect("failed to run ls");
	/// ```
	pub fn stdout<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> Sio2jailExecutor<'a> {
		self.stdout_fd = Some(fd.into());
		self
	}

	/// Sets the file descriptor for the child process’s standard error (stderr)
	///
	/// If this function is not called, child stderr is inherited from the parent process
	///
	/// # Examples
	///
	/// Basic usage:
	///
	/// ```
	/// use libsio2jail::process::Sio2jailExecutor;
	/// use std::fs::File;
	/// use std::os::fd::AsFd;
	///
	/// let file = File::open("/dev/null").unwrap();
	///
	/// Sio2jailExecutor::new("ls")
	///     .stderr(file.as_fd())
	///     .spawn()
	///     .expect("failed to spawn child")
	///     .run()
	///     .expect("failed to run ls");
	/// ```
	pub fn stderr<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> Sio2jailExecutor<'a> {
		self.stderr_fd = Some(fd.into());
		self
	}

	/// Adds feature flags to influence how program execution is sandboxed and measured
	///
	/// Multiple features can be added at once if they are separated by the `|` character
	///
	/// # Examples
	///
	/// Basic usage:
	///
	/// ```
	/// use libsio2jail::process::Feature::{PERF, SECCOMP};
	/// use libsio2jail::process::Sio2jailExecutor;
	///
	/// Sio2jailExecutor::new("ls")
	///     .features(PERF | SECCOMP)
	///     .spawn()
	///     .expect("failed to spawn child")
	///     .run()
	///     .expect("failed to run ls");
	/// ```
	pub fn features<T: Into<EnumSet<Feature>>>(mut self, features: T) -> Sio2jailExecutor<'a> {
		self.features.insert_all(features.into());
		self
	}

	/// Sets a limit on how much real time can pass after the child program is executed
	/// before it is killed and [`ExitStatus::TLE`](crate::process::ExitStatus::TLE) is
	/// returned as the exit status
	///
	/// # Examples
	///
	/// Basic usage:
	/// 
	/// ```
	/// use std::time::Duration;
	/// use libsio2jail::process::ExitStatus::TLE;
	/// use libsio2jail::process::Sio2jailExecutor;
	///
	/// let result = Sio2jailExecutor::new("sleep")
	///     .arg("1")
	///     .real_time_limit(Duration::from_secs_f64(0.5))
	///     .spawn()
	///     .expect("failed to spawn child")
	///     .run()
	///     .expect("failed to run sleep");
	///
	/// assert!(matches!(1, 1));
	/// ```
	pub fn real_time_limit(mut self, limit: Duration) -> Sio2jailExecutor<'a> {
		self.real_time_limit = Some(limit);
		self
	}

	pub fn measured_time_limit(mut self, limit: Duration) -> Sio2jailExecutor<'a> {
		self.instruction_count_limit =
			Some((limit.as_millis() * ((CYCLES_PER_SECOND / 1_000) as u128)) as i64);
		self = self.features(PERF);
		self
	}

	pub fn spawn(self) -> io::Result<Sio2jailChild<'a>> {
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

		Ok(Sio2jailChild::new(context, child_stack))
	}
}
