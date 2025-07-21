mod sighandler;

use crate::listener::perf::sighandler::SIGHANDLER_STATE;
use crate::listener::Listener;
use crate::process::data::{ExecutionData, ExecutionSettings};
use crate::process::error::RunError;
use crate::process::{ExecuteAction, ExitStatus};
use crate::util::errno;
use cvt::cvt;
use libc::{__u64, c_int, fcntl, getpid, read, F_GETFL, F_SETFL, F_SETOWN, O_ASYNC, SIGRTMIN};
use linux_raw_sys::general::F_SETSIG;
use perf_event_open_sys::bindings::{
    perf_event_attr, PERF_COUNT_HW_INSTRUCTIONS, PERF_FLAG_FD_CLOEXEC, PERF_FLAG_FD_NO_GROUP,
    PERF_TYPE_HARDWARE,
};
use perf_event_open_sys::perf_event_open;
use std::ffi::{c_long, c_ulong, c_void};
use std::io::Read;
use std::mem::{size_of_val, zeroed};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::Barrier;

#[derive(Debug)]
pub(crate) struct PerfListener {
    barrier: Barrier,
    perf_fd: Option<OwnedFd>,
    read_stream: Option<UnixStream>,
}

impl PerfListener {
    pub(crate) fn new() -> PerfListener {
        sighandler::init_sighandler();

        PerfListener {
            barrier: Barrier::new(2),
            perf_fd: None,
            read_stream: None,
        }
    }
}

impl Listener for PerfListener {
    fn get_poll_fds(&'_ mut self) -> Vec<BorrowedFd<'_>> {
        if let Some(stream) = &self.read_stream {
            vec![stream.as_fd()]
        }
        else {
            vec![]
        }
    }

    fn on_post_fork_child(
        &mut self,
        _: &ExecutionSettings,
        _: &ExecutionData,
    ) -> Result<(), RunError> {
        self.barrier.wait();

        Ok(())
    }

    fn on_post_fork_parent(&mut self, _settings: &ExecutionSettings, data: &mut ExecutionData) {
        unsafe {
            let mut attrs: perf_event_attr = zeroed();
            attrs.type_ = PERF_TYPE_HARDWARE;
            attrs.config = PERF_COUNT_HW_INSTRUCTIONS as __u64;
            attrs.size = size_of_val(&attrs) as u32;
            attrs.set_exclude_user(0);
            attrs.set_exclude_kernel(1);
            attrs.set_exclude_hv(1);
            attrs.set_disabled(1);
            attrs.set_enable_on_exec(1);
            attrs.set_inherit(1);

            if let Some(limit) = _settings.instruction_count_limit {
                attrs.__bindgen_anon_1.sample_period = limit as __u64;
                attrs.__bindgen_anon_2.wakeup_events = 1;
            }

            let perf_fd = cvt(perf_event_open(
                &mut attrs,
                data.pid.unwrap(),
                -1,
                -1,
                (PERF_FLAG_FD_NO_GROUP | PERF_FLAG_FD_CLOEXEC) as c_ulong,
            )).unwrap();
            self.perf_fd = Some(OwnedFd::from_raw_fd(perf_fd));

            if _settings.instruction_count_limit.is_some() {
                cvt(fcntl(perf_fd, F_SETOWN, getpid())).unwrap();
                let old_flags = cvt(fcntl(perf_fd, F_GETFL, 0)).unwrap();
                cvt(fcntl(perf_fd, F_SETFL, old_flags | O_ASYNC)).unwrap();
                cvt(fcntl(perf_fd, F_SETSIG as c_int, SIGRTMIN())).unwrap();
            }

            let (read, write) = UnixStream::pair().unwrap();
            write.set_nonblocking(true).unwrap();
            read.set_nonblocking(true).unwrap();
            (&*SIGHANDLER_STATE).perf_fd_map.insert(perf_fd, write).unwrap();
            self.read_stream = Some(read);

            self.barrier.wait();
        }
    }

    fn on_post_execute(&mut self, _: &ExecutionSettings, data: &mut ExecutionData) {
        data.execution_result
            .set_instructions_used(self.get_instructions_used());

        if let Some(perf_fd) = &self.perf_fd {
            unsafe {
                (&*SIGHANDLER_STATE).perf_fd_map.remove(&perf_fd.as_raw_fd());
            }
        }
    }

    fn on_wakeup(
        &mut self,
        settings: &ExecutionSettings,
        data: &mut ExecutionData,
    ) -> (ExecuteAction, Option<i32>) {
        if let Some(instruction_count_limit) = settings.instruction_count_limit {
            let mut buf = [0u8; 1024];
            _ = self.read_stream.as_ref().unwrap().read(&mut buf);

            let instructions_used = self.get_instructions_used();

            if instructions_used > instruction_count_limit {
                data.execution_result
                    .set_exit_status(ExitStatus::TLE("time limit exceeded".into()));
                (ExecuteAction::Kill, None)
            } else {
                (ExecuteAction::Continue, None)
            }
        } else {
            (ExecuteAction::Continue, None)
        }
    }
}

impl PerfListener {
    fn get_instructions_used(&mut self) -> i64 {
        let mut instructions_used: i64 = 0;

        unsafe {
            let size = read(
                self.perf_fd.as_ref().unwrap().as_raw_fd(),
                &mut instructions_used as *mut c_long as *mut c_void,
                size_of_val(&instructions_used),
            );

            if size != size_of_val(&instructions_used) as isize {
                panic!("ERROR {} {}\n\n", size, errno());
            }
            if instructions_used < 0 {
                panic!("ERROR2");
            }
        }

        instructions_used
    }
}