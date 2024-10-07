use crate::listener::Listener;
use crate::process::data::{ExecutionData, ExecutionSettings};
use crate::process::error::RunError;
use crate::process::{ExecuteAction, ExitStatus};
use crate::util::errno;
use libc::{
    __u64

    , read,
};
use perf_event_open_sys::bindings::{
    perf_event_attr, PERF_COUNT_HW_INSTRUCTIONS, PERF_FLAG_FD_CLOEXEC, PERF_FLAG_FD_NO_GROUP,
    PERF_TYPE_HARDWARE,
};
use perf_event_open_sys::perf_event_open;
use std::ffi::{c_long, c_ulong, c_void};
use std::mem::{size_of_val, zeroed};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Barrier;

#[derive(Debug)]
pub(crate) struct PerfListener {
    barrier: Barrier,
    perf_fd: Option<OwnedFd>,
}

impl PerfListener {
    pub(crate) fn new() -> PerfListener {
        PerfListener {
            barrier: Barrier::new(2),
            perf_fd: None,
        }
    }
}

impl Listener for PerfListener {
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

            let perf_fd = perf_event_open(
                &mut attrs,
                data.pid.unwrap(),
                -1,
                -1,
                (PERF_FLAG_FD_NO_GROUP | PERF_FLAG_FD_CLOEXEC) as c_ulong,
            );
            self.perf_fd = Some(OwnedFd::from_raw_fd(perf_fd));
        }
        
        self.barrier.wait();
    }

    fn on_post_execute(&mut self, _: &ExecutionSettings, data: &mut ExecutionData) {
        data.execution_result
            .set_instructions_used(self.get_instructions_used());
    }

    fn on_wakeup(
        &mut self,
        settings: &ExecutionSettings,
        data: &mut ExecutionData,
    ) -> (ExecuteAction, Option<i32>) {
        if let Some(instruction_count_limit) = settings.instruction_count_limit {
            let instructions_used = self.get_instructions_used();

            if instructions_used > instruction_count_limit {
                data.execution_result
                    .set_exit_status(ExitStatus::TLE("time limit exceeded".into()));
                (ExecuteAction::Kill, None)
            } else {
                (ExecuteAction::Continue, Some(1))
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
