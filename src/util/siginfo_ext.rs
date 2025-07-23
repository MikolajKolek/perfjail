#![allow(non_camel_case_types)]

use cfg_if::cfg_if;
use libc::{c_long, siginfo_t};
use std::ffi::{c_int, c_void};

cfg_if! {
    if #[cfg(any(
        target_arch = "sparc",
        target_arch = "sparc64",
    ))] {
        pub type __SI_BAND_TYPE = c_int;
    } else {
        pub type __SI_BAND_TYPE = c_long;
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
struct sifields_sigio {
    si_band: __SI_BAND_TYPE,
    si_fd: c_int,
}

#[repr(C)]
union sifields {
    _align_pointer: *mut c_void,
    sigio: sifields_sigio,
}

#[repr(C)]
struct siginfo_f {
    _siginfo_base: [libc::c_int; 3],
    sifields: sifields,
}

pub(crate) trait siginfo_t_ext {
    unsafe fn si_band(&self) -> __SI_BAND_TYPE;

    unsafe fn si_fd(&self) -> c_int;
}

impl siginfo_t_ext for siginfo_t {
    unsafe fn si_band(&self) -> __SI_BAND_TYPE {
        (*(self as *const siginfo_t as *const siginfo_f)).sifields.sigio.si_band
    }

    unsafe fn si_fd(&self) -> c_int {
        (*(self as *const siginfo_t as *const siginfo_f)).sifields.sigio.si_fd
    }
}