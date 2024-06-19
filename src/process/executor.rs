use cvt::cvt;
use std::ffi::{c_int, c_void, CString, OsStr};
use std::io;
use std::os::fd::{BorrowedFd, FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::time::Duration;

use enumset::{EnumSet, EnumSetType};
use libc::{clone, malloc, CLONE_PIDFD, CLONE_VM, SIGCHLD};

use crate::listener::perf::PerfListener;
use crate::listener::Listener;
use crate::process::child::{execute_child, Sio2jailChild};
use crate::process::data::{ExecutionContext, ExecutionData, ExecutionSettings};
use crate::process::executor::Feature::PERF;
use crate::util::{CHILD_STACK_SIZE, CYCLES_PER_SECOND};

#[allow(dead_code)]
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

#[derive(EnumSetType, Debug)]
pub enum Feature {
	PERF,
}

#[allow(dead_code)]
impl<'a> Sio2jailExecutor<'a> {
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

	pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Sio2jailExecutor<'a> {
		self.args.push(
			CString::new(arg.as_ref().as_encoded_bytes())
				.expect("Failed to convert program arg to CString"),
		);
		self
	}

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

	pub fn current_dir(mut self, dir: PathBuf) -> Sio2jailExecutor<'a> {
		self.working_dir = dir;
		self
	}

	pub fn stdin<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> Sio2jailExecutor<'a> {
		self.stdin_fd = Some(fd.into());
		self
	}

	pub fn stdout<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> Sio2jailExecutor<'a> {
		self.stdout_fd = Some(fd.into());
		self
	}

	pub fn stderr<T: Into<BorrowedFd<'a>>>(mut self, fd: T) -> Sio2jailExecutor<'a> {
		self.stderr_fd = Some(fd.into());
		self
	}

	pub fn feature(mut self, feature: Feature) -> Sio2jailExecutor<'a> {
		self.features.insert(feature);
		self
	}

	pub fn features(mut self, features: EnumSet<Feature>) -> Sio2jailExecutor<'a> {
		self.features.insert_all(features);
		self
	}

	pub fn real_time_limit(mut self, limit: Duration) -> Sio2jailExecutor<'a> {
		self.real_time_limit = Some(limit);
		self
	}

	pub fn measured_time_limit(mut self, limit: Duration) -> Sio2jailExecutor<'a> {
		self.instruction_count_limit =
			Some((limit.as_millis() * ((CYCLES_PER_SECOND / 1_000) as u128)) as i64);
		self = self.feature(PERF);
		self
	}

	pub fn spawn(self) -> io::Result<Sio2jailChild<'a>> {
		let listeners = self
			.features
			.iter()
			.map(|feature| match feature {
				PERF => Box::new(PerfListener::new()) as Box<dyn Listener>,
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
