use crate::util::fixed_map::FixedMap;
use crate::util::siginfo_ext::siginfo_t_ext;
use libc::c_int;
use std::io::Write;
use std::mem;
use std::mem::zeroed;
use std::os::unix::net::UnixStream;
use std::ptr::null_mut;
use std::sync::atomic::AtomicBool;

pub(crate) struct SighandlerState {
    previous_sigrtmin_handler: libc::sigaction,
    previous_sigio_handler: libc::sigaction,
    pub(crate) perf_fd_map: FixedMap
}

pub(crate) static SIGHANDLER_INIT_STARTED: AtomicBool = AtomicBool::new(false);
pub(crate) static mut SIGHANDLER_STATE: *mut SighandlerState = null_mut();


fn init(perf_timeout_thread_count: usize) {
    unsafe {
        let state = Box::new(
            SighandlerState {
                previous_sigrtmin_handler: zeroed(),
                previous_sigio_handler: zeroed(),
                perf_fd_map: FixedMap::new(perf_timeout_thread_count * 10, perf_timeout_thread_count),
            }
        );
        SIGHANDLER_STATE = Box::into_raw(state);

        let mut sigrtmin: libc::sigaction = zeroed();
        sigrtmin.sa_sigaction = sigrtmin_handler as usize;
        sigrtmin.sa_flags = libc::SA_RESTART | libc::SA_SIGINFO;
        assert_eq!(libc::sigaction(libc::SIGRTMIN(), &sigrtmin, &mut (*SIGHANDLER_STATE).previous_sigrtmin_handler), 0);

        let mut sigio: libc::sigaction = zeroed();
        sigio.sa_sigaction = sigio_handler as usize;
        sigio.sa_flags = libc::SA_RESTART | libc::SA_SIGINFO;
        assert_eq!(libc::sigaction(libc::SIGIO, &sigio, &mut (*SIGHANDLER_STATE).previous_sigio_handler), 0);
    }
}

pub(crate) fn init_sighandler(perf_timeout_thread_count: usize) -> bool {
    if SIGHANDLER_INIT_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        false
    }
    else {
        init(perf_timeout_thread_count);
        true
    }
}


type FnSigaction = extern "C" fn(c_int, *mut libc::siginfo_t, *mut libc::c_void);
type FnHandler = extern "C" fn(c_int);

fn notify(mut stream: &UnixStream) {
    match stream.write(&[1]) {
        Ok(_) => {}
        Err(e) => {
            if e.kind() != std::io::ErrorKind::WouldBlock {
                panic!("Error writing to perf_fd_map fd: {}", e);
            }
        }
    }
}

unsafe fn call_previous_handler(
    signum: c_int,
    info: *mut libc::siginfo_t,
    ptr: *mut libc::c_void,
    previous_handler: libc::sigaction
) {
    let fnptr = previous_handler.sa_sigaction;
    if fnptr == 0 {
        return;
    }

    if previous_handler.sa_flags & libc::SA_SIGINFO == 0 {
        let action = mem::transmute::<usize, FnHandler>(fnptr);
        action(signum)
    } else {
        let action = mem::transmute::<usize, FnSigaction>(fnptr);
        action(signum, info, ptr)
    }
}

unsafe extern "C" fn sigrtmin_handler(signum: c_int, info: *mut libc::siginfo_t, ptr: *mut libc::c_void) {
    let state = &*SIGHANDLER_STATE;

    state.perf_fd_map.get_and_write((*info).si_fd());

    call_previous_handler(signum, info, ptr, state.previous_sigrtmin_handler);
}


unsafe extern "C" fn sigio_handler(signum: c_int, info: *mut libc::siginfo_t, ptr: *mut libc::c_void) {
    let state = &*SIGHANDLER_STATE;

    //TODO: fix by implementing something that can do scan
    /*state.perf_fd_map.scan(|_, v| {
        notify(v);
    });*/
    //panic!("AAAAA");

    call_previous_handler(signum, info, ptr, state.previous_sigio_handler);
}