# libsio2jail
[![Crates.io](https://img.shields.io/crates/l/libsio2jail)](https://github.com/MikolajKolek/libsio2jail/blob/master/LICENSE)
[![Crates.io](https://img.shields.io/crates/d/libsio2jail)](https://crates.io/crates/libsio2jail)
[![Crates.io](https://img.shields.io/crates/v/libsio2jail)](https://crates.io/crates/libsio2jail)
[![Libsio2jail documentation](https://docs.rs/libsio2jail/badge.svg)](https://docs.rs/libsio2jail)

A Rust reimplementation of sio2jail - a tool for supervising the execution of programs submitted in algorithmic competitions

Sio2jail is primarily used by the [Polish Olympiad in Informatics](https://www.oi.edu.pl/) for providing fair time and memory use measurements for problem solutions and for sandboxing

Currently, the library doesn't include many of sio2jail's sandboxing features, as it was made primarily for quick testing of trusted programs, but as the project is further updated, those features may be reimplemented

The project is also currently very much a work-in-progress, with messy, undocumented code without error handling, but all that will change before the first release

# License
Libsio2jail is licensed under the [MIT Licence](https://github.com/MikolajKolek/toster/blob/master/LICENSE)

The project is based on [sio2jail](https://github.com/sio2project/sio2jail), which is also available under the MIT license
