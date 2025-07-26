use crate::listener::{Listener, WakeupAction};
use crate::process::data::{ExecutionData, ExecutionSettings};
use crate::process::ExitStatus;
use cvt::{cvt, cvt_r};
use libc::{__u64, read};
use perf_event_open_sys::bindings::{
    perf_event_attr, PERF_COUNT_HW_INSTRUCTIONS, PERF_FLAG_FD_CLOEXEC, PERF_FLAG_FD_NO_GROUP,
    PERF_TYPE_HARDWARE,
};
use perf_event_open_sys::perf_event_open;
use std::ffi::{c_long, c_ulong, c_void};
use std::io;
use std::mem::{size_of_val, zeroed};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use nix::sys::wait::WaitStatus;

#[derive(Debug)]
pub(crate) struct PerfListener {
    perf_fd: Option<OwnedFd>,
}

impl PerfListener {
    pub(crate) fn new() -> PerfListener {
        PerfListener {
            perf_fd: None,
        }
    }
}

impl Listener for PerfListener {
    fn requires_timeout(&self, settings: &ExecutionSettings) -> bool {
        settings.instruction_count_limit.is_some()
    }

    fn on_post_clone_child(
        &self,
        _: &ExecutionSettings,
        _: &ExecutionData,
    ) -> io::Result<()> {
        Ok(())
    }

    fn on_post_clone_parent(&mut self, _settings: &ExecutionSettings, data: &mut ExecutionData) -> io::Result<()> {
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

            let perf_fd = cvt(perf_event_open(
                &mut attrs,
                data.pid.unwrap(),
                -1,
                -1,
                (PERF_FLAG_FD_NO_GROUP | PERF_FLAG_FD_CLOEXEC) as c_ulong,
            ))?;
            self.perf_fd = Some(OwnedFd::from_raw_fd(perf_fd));
        }
        
        Ok(())
    }

    fn on_wakeup(
        &mut self,
        settings: &ExecutionSettings,
        data: &mut ExecutionData,
    ) -> io::Result<WakeupAction>{
        if let Some(instruction_count_limit) = settings.instruction_count_limit {
            let instructions_used = self.get_instructions_used()?;

            if instructions_used > instruction_count_limit {
                data.execution_result
                    .set_exit_status(ExitStatus::TLE("time limit exceeded".into()));
                Ok(WakeupAction::Kill)
            } else {
                Ok(WakeupAction::Continue)
            }
        } else {
            Ok(WakeupAction::Continue)
        }
    }

    fn on_execute_event(
        &mut self,
        _: &ExecutionSettings,
        _: &mut ExecutionData,
        _: &WaitStatus
    ) -> io::Result<WakeupAction> {
        Ok(WakeupAction::Continue)
    }

    fn on_post_execute(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData) -> io::Result<()> {
        let instructions_used = self.get_instructions_used()?;

        if let Some(instruction_limit) = settings.instruction_count_limit {
            if instructions_used > instruction_limit {
                data.execution_result
                    .set_exit_status(ExitStatus::TLE("time limit exceeded".into()));
            }
        }

        data.execution_result.set_instructions_used(instructions_used);
        Ok(())
    }
}

impl PerfListener {
    fn get_instructions_used(&mut self) -> io::Result<i64> {
        let mut instructions_used: i64 = 0;
        
        unsafe {
            let bytes_read = cvt_r(|| {
                read(
                    self.perf_fd.as_ref().unwrap().as_raw_fd(),
                    &mut instructions_used as *mut c_long as *mut c_void,
                    size_of_val(&instructions_used),
                )
            })?;

            if bytes_read != size_of_val(&instructions_used) as isize {
                panic!("Read returned fewer bytes than requested ({} / {})", bytes_read, size_of_val(&instructions_used));
            }
            if instructions_used < 0 {
                panic!("Read returned negative number of instructions used: {instructions_used}");
            }
        }

        Ok(instructions_used)
    }
}
