#![warn(missing_docs)]
//! A Rust reimplementation sio2jail - a tool for supervising the execution of programs submitted in algorithmic competitions

mod listener;
mod util;
/// Utilities for setting up Linux for using libsio2jail with perf
pub mod perf;
/// A module for creating and managing libsio2jail processes
pub mod process;

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::time::Duration;
    use crate::process::execution_result::ExitResult::Exited;
    use crate::process::executor::Feature::PERF;
    use crate::process::executor::Sio2jailExecutor;

    #[test]
    fn time_measurement_test() {
        //TODO: COMPREHENSIVE UNIT TESTING SYSTEM

        let child = Sio2jailExecutor::new("tests/bud")
            .stdin(File::open("tests/bud.in").unwrap())
            .stdout(File::create("tests/test_output.out").unwrap())
            .feature(PERF)
            .measured_time_limit(Duration::from_millis(450))
            .spawn().unwrap();
        let result = child.run().unwrap();
        println!("Exit result: {:?}", result);
        let Exited { exit_status: status } = result.exit_result else { panic!("not supposed to happen") };
        println!("Exit status: {}, measured time: {}", status, result.measured_time.unwrap().as_millis());

        assert_eq!(result.measured_time.unwrap().as_millis(), 458);
    }
}
