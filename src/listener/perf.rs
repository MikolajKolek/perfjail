use crate::listener::{Listener, WakeupAction};
use crate::process::data::{ExecutionData, ExecutionSettings};
use cvt::cvt;
use libc::{__u64, read, ssize_t};
use perf_event_open_sys::bindings::{
    perf_event_attr, PERF_COUNT_HW_INSTRUCTIONS, PERF_FLAG_FD_CLOEXEC, PERF_FLAG_FD_NO_GROUP,
    PERF_TYPE_HARDWARE,
};
use perf_event_open_sys::perf_event_open;
use std::ffi::{c_long, c_ulong, c_void};
use std::io;
use std::mem::{size_of_val, zeroed};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Barrier;
use crate::process::ExitStatus;

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
    fn on_post_clone_child(
        &mut self,
        _: &ExecutionSettings,
        _: &ExecutionData,
    ) -> io::Result<()> {
        self.barrier.wait();
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
        
        self.barrier.wait();
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
                Ok(WakeupAction::Continue { next_wakeup: Some(1) })
            }
        } else {
            Ok(WakeupAction::Continue { next_wakeup: None })
        }
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
            let bytes_read: ssize_t;
            let mut iterations= 0;
            loop {
                iterations += 1;
                if iterations > 10 {
                    panic!("Read loop was interrupted too many times");
                }

                let result = cvt(read(
                    self.perf_fd.as_ref().unwrap().as_raw_fd(),
                    &mut instructions_used as *mut c_long as *mut c_void,
                    size_of_val(&instructions_used),
                ));

                if let Err(e) = result {
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    } else {
                        return Err(e);
                    }
                }
                else if let Ok(result) = result {
                    bytes_read = result;
                    break;
                }
            }

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
