use crate::listener::{Listener, WakeupAction};
use crate::process::data::{ExecutionData, ExecutionSettings};
use crate::process::ExitStatus;
use cvt::cvt;
use libc::{sysconf, _SC_CLK_TCK};
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
pub(crate) struct TimeLimitListener {
    real_time_start: Option<Instant>,
    time_limit_set: bool
}

impl TimeLimitListener {
    pub(crate) fn new() -> TimeLimitListener {
        CLOCK_TICKS_PER_SECOND.get_or_init(|| {
            (unsafe { cvt(sysconf(_SC_CLK_TCK)).expect("Failed to read _SC_CLK_TCK") } as u64)
        });

        TimeLimitListener {
            real_time_start: None,
            time_limit_set: false
        }
    }
}

impl Listener for TimeLimitListener {
    fn requires_timeout(&self, settings: &ExecutionSettings) -> bool {
        settings.real_time_limit.is_some() ||
        settings.user_time_limit.is_some() ||
        settings.system_time_limit.is_some() ||
        settings.user_system_time_limit.is_some()
    }

    fn on_post_clone_child(&mut self, _: &ExecutionSettings, _: &ExecutionData) -> io::Result<()> {
        Ok(())
    }

    fn on_post_clone_parent(&mut self, settings: &ExecutionSettings, _: &mut ExecutionData) -> io::Result<()> {
        // Sio2jail also sets this value here, even if it's slightly inaccurate.
        self.real_time_start = Some(Instant::now());

        self.time_limit_set =
            settings.real_time_limit.is_some() ||
            settings.user_time_limit.is_some() ||
            settings.system_time_limit.is_some() ||
            settings.user_system_time_limit.is_some();

        Ok(())
    }

    fn on_wakeup(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData) -> io::Result<WakeupAction> {
        if !self.time_limit_set {
            Ok(WakeupAction::Continue)
        } else {
            Ok(self.verify_time_usage(settings, data, self.get_time_usage(data)?))
        }
    }

    fn on_post_execute(&mut self, settings: &ExecutionSettings, data: &mut ExecutionData) -> io::Result<()> {
        let time_usage = self.get_time_usage(data)?;

        data.execution_result.set_real_time(time_usage.real_time);
        data.execution_result.set_user_time(time_usage.process_time_usage.user_time);
        data.execution_result.set_system_time(time_usage.process_time_usage.system_time);

        if self.time_limit_set {
            self.verify_time_usage(settings, data, time_usage);
        }

        Ok(())
    }
}

impl TimeLimitListener {
    /// This function can only be called when at least one of the time limits is set
    fn verify_time_usage(
        &self,
        settings: &ExecutionSettings,
        data: &mut ExecutionData,
        time_usage: TimeUsage
    ) -> WakeupAction {
        if let Some(limit) = settings.real_time_limit
            && time_usage.real_time > limit {

            data.execution_result.set_exit_status(ExitStatus::TLE("real time limit exceeded".into()));
            WakeupAction::Kill
        } else if let Some(limit) = settings.user_time_limit &&
            time_usage.process_time_usage.user_time > limit {

            data.execution_result.set_exit_status(ExitStatus::TLE("user time limit exceeded".into()));
            WakeupAction::Kill
        } else if let Some(limit) = settings.system_time_limit &&
            time_usage.process_time_usage.system_time > limit {

            data.execution_result.set_exit_status(ExitStatus::TLE("system time limit exceeded".into()));
            WakeupAction::Kill
        } else if let Some(limit) = settings.user_system_time_limit &&
            time_usage.process_time_usage.user_time + time_usage.process_time_usage.system_time > limit {

            data.execution_result.set_exit_status(ExitStatus::TLE("user+system time limit exceeded".into()));
            WakeupAction::Kill
        } else {
            WakeupAction::Continue
        }
    }

    fn get_process_time_usage(&self, data: &ExecutionData) -> io::Result<ProcessTimeUsage> {
        let stat = fs::read_to_string(format!("/proc/{}/stat", data.pid.expect("pid not set")))?;
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
        Instant::now() - self.real_time_start.expect("real_time_start not set")
    }

    fn get_time_usage(&self, data: &ExecutionData) -> io::Result<TimeUsage> {
        Ok(TimeUsage {
            process_time_usage: self.get_process_time_usage(data)?,
            real_time: self.get_real_time_usage(),
        })
    }
}