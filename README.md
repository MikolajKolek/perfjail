# perfjail
[![Crates.io](https://img.shields.io/crates/l/perfjail)](https://github.com/MikolajKolek/perfjail/blob/master/LICENSE)
[![Crates.io](https://img.shields.io/crates/d/perfjail)](https://crates.io/crates/perfjail)
[![Crates.io](https://img.shields.io/crates/v/perfjail)](https://crates.io/crates/perfjail)
[![Perfjail documentation](https://docs.rs/perfjail/badge.svg)](https://docs.rs/perfjail)

A library for supervising the execution of programs in algorithmic competitions, inspired by [sio2jail](https://github.com/sio2project/sio2jail) - a tool used by the Polish Olympiad in Informatics. It is however much faster than sio2jail for small tests, as it changes the [`fork`](https://man7.org/linux/man-pages/man2/fork.2.html) syscall before the execution of the tested program into a [`clone`](https://man7.org/linux/man-pages/man2/clone.2.html) with the `CLONE_VM` flag set, thus eliminating the need for preparing the copy-on-write data which fork usually uses. As it turns out, that `fork` call is the most expensive part of sio2jail execution for small tests, allowing perfjail to be significantly (up to 30x in my testing!) faster.

Perfjail can be used for providing fair time and memory use measurements for problem solutions and for sandboxing

Currently, the library doesn't include many of sio2jail's sandboxing features, as it was made primarily for fast testing of trusted programs, but as the project is further updated, those features may be reimplemented

The project is also currently very much a work-in-progress, with messy, undocumented code without error handling, but all that will change before the first full release

# License
Perfjail is licensed under the [MIT Licence](https://github.com/MikolajKolek/perfjail/blob/master/LICENSE) 

Some of the project's code is based on sio2jail, which is also available under the MIT license

The comments and basic structure for the `Perfjail` struct are based on [`std::process::Command`](https://doc.rust-lang.org/std/process/struct.Command.html) from the [Rust standard library](https://github.com/rust-lang/rust), which is also available under the MIT license
