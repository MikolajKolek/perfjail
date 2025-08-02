#![feature(unsafe_cell_access)]
#![warn(missing_docs)]
//! A library for supervising the execution of programs in algorithmic competitions, inspired by sio2jail - a tool used by the Polish Olympiad in Informatics

/// Utilities for creating and managing perfjail processes
pub mod process;
/// Utilities for setting Linux up for perfjail use
pub mod setup;

mod listener;
mod util;

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::fd::AsFd;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use crate::process::execution_result::ExitReason::Exited;
    use crate::process::{ExecutionResult, ExitReason, ExitStatus};
    use crate::process::Feature::{MEMORY_MEASUREMENT, TIME_MEASUREMENT};
    use crate::process::jail::Feature::PERF;
    use crate::process::jail::Perfjail;

    #[test]
    fn time_measurement_test() {
        //TODO: COMPREHENSIVE UNIT TESTING SYSTEM

        let input_file = File::open("tests/bud.in").unwrap();
        let output_file = File::create("tests/test_output.out").unwrap();

        let child = Perfjail::new("tests/bud")
            .stdin(input_file.as_fd())
            .stdout(output_file.as_fd())
            .features(PERF | TIME_MEASUREMENT | MEMORY_MEASUREMENT)
            .measured_time_limit(Duration::from_millis(500))
            .spawn()
            .unwrap();
        let result = child.run().unwrap();

        println!("Exit result: {:?}", result);
        let Exited {
            exit_status: status,
        } = result.exit_reason
        else {
            panic!("not supposed to happen")
        };
        println!(
            "Exit status: {}, measured time: {}",
            status,
            result.measured_time.unwrap().as_millis()
        );

        assert_eq!(result.measured_time.unwrap().as_millis(), 467);
    }

    #[test]
    fn concurrent_kill_run_test() {
        let child = Arc::new(
            Perfjail::new("sleep")
                .arg("0.1")
                .spawn()
                .unwrap()
        );

        let child_clone = Arc::clone(&child);
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            child_clone.kill().unwrap();
        });

        let result = child.run().unwrap();
        handle.join().unwrap();

        assert_eq!(result.exit_status, ExitStatus::RE("runtime error: killed by signal 9".into()));
        assert_eq!(result.exit_reason, ExitReason::Killed { signal: 9 });
        assert_eq!(result.instructions_used, None);
        assert_eq!(result.measured_time, None);
    }

    #[test]
    fn concurrent_run_test() {
        let child = Arc::new(
            Perfjail::new("sleep")
                .arg("0.1")
                .spawn()
                .unwrap()
        );
        let child_result: Arc<Mutex<ExecutionResult>> = Arc::new(Mutex::new(ExecutionResult::new()));

        let child_clone = Arc::clone(&child);
        let child_result_clone = Arc::clone(&child_result);
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            let mut guard = child_result_clone.lock().unwrap();
            *guard = child_clone.run().unwrap();
        });

        let result = child.run().unwrap();
        handle.join().unwrap();

        assert_eq!(*child_result.lock().unwrap(), result);
    }
}
