use std::io;
use libc::{c_int, pid_t, size_t, SIGKILL};
use std::io::Error;
use cvt::cvt;

/// The stack size (in bytes) for creating the child process with [`clone`].
///
/// The standard Linux process stack size is usually 8MB, but the process we create consumes practically no memory, so the stack size can be greatly decreased here.
pub(crate) const CHILD_STACK_SIZE: size_t = 65536;

/// The number of CPU cycles perfjail considers equal to 1 second in measured time.
pub(crate) const CYCLES_PER_SECOND: i64 = 2_000_000_000;

pub(crate) fn errno() -> i32 {
    Error::last_os_error().raw_os_error().unwrap()
}

pub(crate) fn cvt_no_errno(argument: c_int) -> io::Result<()> {
    if argument == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(argument))
    }
}

pub(crate) fn kill_pid(pid: pid_t) -> io::Result<()> {
    unsafe {
        cvt(libc::kill(pid, SIGKILL)).map(|_| ())
    }
}
