use crate::listener::{Listener, WakeupAction};
use crate::process::data::{ExecutionContext, ExecutionSettings, ParentData};
use crate::process::ExitStatus;
use cvt::cvt;
use libc::{c_int, sysconf, _SC_CLK_TCK};
use nix::sys::wait::WaitStatus;
use std::cell::UnsafeCell;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use std::{fs, io};

static CLOCK_TICKS_PER_SECOND: OnceLock<u64> = OnceLock::new();

struct TimeUsage {
    process_time_usage: ProcessTimeUsage,
    real_time: Duration,
}

struct ProcessTimeUsage {
    user_time: Duration,
    system_time: Duration,
}

#[derive(Debug)]
pub(crate) struct TimeListener {
    real_time_start: UnsafeCell<Option<Instant>>,
    time_limit_set: UnsafeCell<bool>
}

impl TimeListener {
    pub(crate) fn new() -> TimeListener {
        CLOCK_TICKS_PER_SECOND.get_or_init(|| {
            (unsafe { cvt(sysconf(_SC_CLK_TCK)).expect("Failed to read _SC_CLK_TCK") } as u64)
        });

        TimeListener {
            real_time_start: UnsafeCell::new(None),
            time_limit_set: UnsafeCell::new(false)
        }
    }
}

impl Listener for TimeListener {
    fn requires_timeout(&self, settings: &ExecutionSettings) -> bool {
        settings.real_time_limit.is_some() ||
        settings.user_time_limit.is_some() ||
        settings.system_time_limit.is_some() ||
        settings.user_system_time_limit.is_some()
    }

    fn on_post_clone_child(&self, _: &ExecutionContext) -> nix::Result<()> {
        Ok(())
    }

    fn on_post_clone_parent(&self, context: &ExecutionContext, _: &mut ParentData) -> io::Result<()> {
        // Sio2jail also sets this value here, even if it's slightly inaccurate.
        unsafe {
            let _ = self.real_time_start.as_mut_unchecked().insert(Instant::now());

            *self.time_limit_set.get() =
                context.settings.real_time_limit.is_some() ||
                    context.settings.user_time_limit.is_some() ||
                    context.settings.system_time_limit.is_some() ||
                    context.settings.user_system_time_limit.is_some();
        }

        Ok(())
    }

    fn on_wakeup(&self, context: &ExecutionContext, parent_data: &mut ParentData) -> io::Result<WakeupAction> {
        if !unsafe { *self.time_limit_set.as_ref_unchecked() } {
            Ok(WakeupAction::Continue)
        } else {
            Ok(self.verify_time_usage(context, parent_data, self.get_time_usage(parent_data.pid)?))
        }
    }

    fn on_execute_event(
        &self,
        _: &ExecutionContext, 
        _: &mut ParentData,
        _: &WaitStatus
    ) -> io::Result<WakeupAction> {
        Ok(WakeupAction::Continue)
    }

    fn on_post_execute(&self, context: &ExecutionContext, parent_data: &mut ParentData) -> io::Result<()> {
        let time_usage = self.get_time_usage(parent_data.pid)?;

        parent_data.execution_result.set_real_time(time_usage.real_time);
        parent_data.execution_result.set_user_time(time_usage.process_time_usage.user_time);
        parent_data.execution_result.set_system_time(time_usage.process_time_usage.system_time);

        if unsafe { *self.time_limit_set.as_ref_unchecked() } {
            self.verify_time_usage(context, parent_data, time_usage);
        }

        Ok(())
    }
}

impl TimeListener {
    /// This function can only be called when at least one of the time limits is set
    fn verify_time_usage(
        &self, 
        context: &ExecutionContext,
        parent_data: &mut ParentData,
        time_usage: TimeUsage
    ) -> WakeupAction {
        if let Some(limit) = context.settings.real_time_limit
            && time_usage.real_time > limit {

            parent_data.execution_result.set_exit_status(ExitStatus::TLE("real time limit exceeded".into()));
            WakeupAction::Kill
        } else if let Some(limit) = context.settings.user_time_limit &&
            time_usage.process_time_usage.user_time > limit {

            parent_data.execution_result.set_exit_status(ExitStatus::TLE("user time limit exceeded".into()));
            WakeupAction::Kill
        } else if let Some(limit) = context.settings.system_time_limit &&
            time_usage.process_time_usage.system_time > limit {

            parent_data.execution_result.set_exit_status(ExitStatus::TLE("system time limit exceeded".into()));
            WakeupAction::Kill
        } else if let Some(limit) = context.settings.user_system_time_limit &&
            time_usage.process_time_usage.user_time + time_usage.process_time_usage.system_time > limit {

            parent_data.execution_result.set_exit_status(ExitStatus::TLE("user+system time limit exceeded".into()));
            WakeupAction::Kill
        } else {
            WakeupAction::Continue
        }
    }

    fn get_process_time_usage(&self, pid: c_int) -> io::Result<ProcessTimeUsage> {
        let stat = fs::read_to_string(format!("/proc/{}/stat", pid))?;
        let mut split_stat = stat.split_whitespace();

        let user_time_ticks = split_stat.nth(13)
            .expect("failed to read user time from /proc/pid/stat").parse::<u64>()
            .expect("failed to parse user time from /proc/pid/stat");
        let system_time_ticks = split_stat.nth(0)
            .expect("failed to read system time from /proc/pid/stat").parse::<u64>()
            .expect("failed to parse system time from /proc/pid/stat");
        let clock_ticks_per_second = CLOCK_TICKS_PER_SECOND.get()
            .expect("failed to read CLOCK_TICKS_PER_SECOND");

        Ok(ProcessTimeUsage {
            user_time: Duration::from_micros((user_time_ticks * 1_000_000) / clock_ticks_per_second),
            system_time : Duration::from_micros((system_time_ticks * 1_000_000) / clock_ticks_per_second),
        })
    }

    fn get_real_time_usage(&self) -> Duration {
        unsafe {
            Instant::now() - self.real_time_start.as_ref_unchecked().unwrap().clone()
        }
    }

    fn get_time_usage(&self, pid: c_int) -> io::Result<TimeUsage> {
        Ok(TimeUsage {
            process_time_usage: self.get_process_time_usage(pid)?,
            real_time: self.get_real_time_usage(),
        })
    }
}